use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::models::LocalKey;

fn ssh_dir() -> Result<PathBuf> {
    let mut dir = dirs::home_dir().context("could not locate home directory")?;
    dir.push(".ssh");
    Ok(dir)
}

fn validate_key_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("key name cannot be empty"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(anyhow!("key name cannot contain path separators"));
    }
    if trimmed.contains('\0') {
        return Err(anyhow!("key name cannot contain null bytes"));
    }
    Ok(trimmed.to_string())
}

pub fn expand_home_path(path: &str) -> Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("path cannot be empty"));
    }

    if trimmed == "~" {
        return dirs::home_dir().context("could not locate home directory");
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        let mut full = dirs::home_dir().context("could not locate home directory")?;
        full.push(rest);
        return Ok(full);
    }

    Ok(PathBuf::from(trimmed))
}

pub fn scan_local_keys() -> Result<Vec<LocalKey>> {
    let ssh_dir = ssh_dir()?;
    fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed ensuring ssh dir exists: {}", ssh_dir.display()))?;

    let mut keys = Vec::new();
    for entry in fs::read_dir(&ssh_dir)
        .with_context(|| format!("failed listing ssh dir: {}", ssh_dir.display()))?
    {
        let entry = entry.context("failed reading directory entry")?;
        let path = entry.path();
        let Some(ext) = path.extension() else {
            continue;
        };

        if ext != "pub" {
            continue;
        }

        let public_key_path = path.to_string_lossy().to_string();
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed reading public key file: {}", path.display()))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (algorithm, comment) = parse_pubkey_metadata(trimmed);
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let private_candidate = path.with_extension("");
        let private_key_path = if private_candidate.exists() {
            Some(private_candidate.to_string_lossy().to_string())
        } else {
            None
        };

        keys.push(LocalKey {
            name,
            algorithm,
            comment,
            fingerprint: fingerprint_for_pubkey(&path).unwrap_or_else(|_| "-".to_string()),
            public_key_path,
            private_key_path,
        });
    }

    keys.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(keys)
}

pub fn generate_ed25519_key(
    name: &str,
    comment: &str,
    passphrase: Option<&str>,
) -> Result<LocalKey> {
    let trimmed_name = validate_key_name(name)?;

    let ssh_dir = ssh_dir()?;
    fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed ensuring ssh dir exists: {}", ssh_dir.display()))?;

    let key_path = ssh_dir.join(&trimmed_name);
    if key_path.exists() || key_path.with_extension("pub").exists() {
        return Err(anyhow!(
            "key already exists at {}",
            key_path.to_string_lossy()
        ));
    }

    let pass = passphrase.unwrap_or("");
    let status = Command::new("ssh-keygen")
        .arg("-q")
        .arg("-t")
        .arg("ed25519")
        .arg("-C")
        .arg(comment)
        .arg("-N")
        .arg(pass)
        .arg("-f")
        .arg(&key_path)
        .status()
        .context("failed to run ssh-keygen")?;

    if !status.success() {
        return Err(anyhow!("ssh-keygen exited with status: {status}"));
    }

    let pub_path = key_path.with_extension("pub");
    let raw = fs::read_to_string(&pub_path).with_context(|| {
        format!(
            "failed reading generated public key: {}",
            pub_path.display()
        )
    })?;
    let (algorithm, parsed_comment) = parse_pubkey_metadata(raw.trim());

    Ok(LocalKey {
        name: trimmed_name,
        algorithm,
        comment: if comment.trim().is_empty() {
            parsed_comment
        } else {
            comment.trim().to_string()
        },
        fingerprint: fingerprint_for_pubkey(&pub_path).unwrap_or_else(|_| "-".to_string()),
        public_key_path: pub_path.to_string_lossy().to_string(),
        private_key_path: Some(key_path.to_string_lossy().to_string()),
    })
}

pub fn import_private_key(
    name: Option<&str>,
    private_key_path: &str,
    passphrase: Option<&str>,
) -> Result<LocalKey> {
    let private_key = expand_home_path(private_key_path)?;
    if !private_key.exists() {
        return Err(anyhow!("private key file does not exist"));
    }

    let mut read_pub = Command::new("ssh-keygen");
    read_pub.arg("-y").arg("-f").arg(&private_key);
    if let Some(pass) = passphrase
        && !pass.is_empty()
    {
        read_pub.arg("-P").arg(pass);
    }

    let output = read_pub
        .output()
        .context("failed reading private key")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ssh-keygen could not read private key: {}",
            stderr.trim()
        ));
    }

    let public_key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if public_key.is_empty() {
        return Err(anyhow!("ssh-keygen returned empty public key"));
    }
    let (algorithm, comment) = parse_pubkey_metadata(&public_key);

    let key_name = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(provided) => validate_key_name(provided)?,
        None => private_key
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("could not derive key name from private key filename"))?,
    };

    let ssh_dir = ssh_dir()?;
    fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed ensuring ssh dir exists: {}", ssh_dir.display()))?;
    let pub_path = ssh_dir.join(format!("{key_name}.pub"));

    if pub_path.exists() {
        let existing = fs::read_to_string(&pub_path)
            .with_context(|| format!("failed reading existing key: {}", pub_path.display()))?;
        if existing.trim() != public_key {
            return Err(anyhow!(
                "key already exists with different contents at {}",
                pub_path.display()
            ));
        }
    } else {
        fs::write(&pub_path, format!("{public_key}\n"))
            .with_context(|| format!("failed writing public key: {}", pub_path.display()))?;
    }

    Ok(LocalKey {
        name: key_name,
        algorithm,
        comment,
        fingerprint: fingerprint_for_pubkey(&pub_path).unwrap_or_else(|_| "-".to_string()),
        public_key_path: pub_path.to_string_lossy().to_string(),
        private_key_path: Some(private_key.to_string_lossy().to_string()),
    })
}

pub fn read_public_key(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed reading public key: {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("public key is empty: {}", path.display()));
    }
    Ok(trimmed.to_string())
}

fn parse_pubkey_metadata(line: &str) -> (String, String) {
    let mut parts = line.split_whitespace();
    let algorithm = parts.next().unwrap_or("unknown").to_string();
    let _key_data = parts.next();
    let comment = parts.collect::<Vec<_>>().join(" ");
    (algorithm, comment)
}

fn fingerprint_for_pubkey(path: &Path) -> Result<String> {
    let output = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(path)
        .output()
        .with_context(|| format!("failed to fingerprint key: {}", path.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "ssh-keygen fingerprint failed for {}",
            path.display()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.split_whitespace();
    let _bits = fields.next();
    let fingerprint = fields
        .next()
        .ok_or_else(|| anyhow!("unexpected ssh-keygen output: {text}"))?;
    Ok(fingerprint.to_string())
}

#[cfg(test)]
mod tests {
    use super::{expand_home_path, parse_pubkey_metadata, validate_key_name};

    #[test]
    fn parses_pubkey_metadata() {
        let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICs comment@example";
        let (alg, comment) = parse_pubkey_metadata(line);
        assert_eq!(alg, "ssh-ed25519");
        assert_eq!(comment, "comment@example");
    }

    #[test]
    fn expands_home_path() {
        let path = expand_home_path("~/example_key").expect("home path expansion failed");
        assert!(path.to_string_lossy().contains("example_key"));
        assert_ne!(path.to_string_lossy(), "~/example_key");
    }

    #[test]
    fn rejects_bad_key_names() {
        assert!(validate_key_name("").is_err());
        assert!(validate_key_name("../bad").is_err());
        assert!(validate_key_name("a/b").is_err());
    }
}
