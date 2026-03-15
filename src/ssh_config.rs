use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::models::{AuthMethod, TargetProfile};

/// A parsed entry from ~/.ssh/config.
#[derive(Clone, Debug)]
pub struct SshConfigEntry {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

impl SshConfigEntry {
    /// Convert this SSH config entry into a `TargetProfile`.
    /// Requires at minimum a hostname and user. Returns `None` if either is missing.
    pub fn to_target_profile(&self) -> Option<TargetProfile> {
        let host = self.hostname.as_ref().or(Some(&self.alias))?;
        let user = self.user.as_ref()?;

        if host.contains('*') || host.contains('?') {
            return None;
        }

        let port = self.port.unwrap_or(22);

        let auth = if let Some(ref identity) = self.identity_file {
            AuthMethod::KeyFile {
                private_key: identity.clone(),
                passphrase: None,
            }
        } else {
            // No identity file specified — cannot create a valid auth method
            // without either a key file or password. We default to key file
            // with the standard path as a convenience.
            AuthMethod::KeyFile {
                private_key: "~/.ssh/id_ed25519".to_string(),
                passphrase: None,
            }
        };

        Some(TargetProfile {
            name: self.alias.clone(),
            host: host.clone(),
            port,
            user: user.clone(),
            auth,
        })
    }
}

/// Parse the SSH config file at the default location (~/.ssh/config).
pub fn parse_ssh_config() -> Result<Vec<SshConfigEntry>> {
    let mut config_path = dirs::home_dir().context("could not locate home directory")?;
    config_path.push(".ssh");
    config_path.push("config");
    parse_ssh_config_from_path(&config_path)
}

/// Parse an SSH config file at an arbitrary path.
pub fn parse_ssh_config_from_path(path: &PathBuf) -> Result<Vec<SshConfigEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed reading SSH config: {}", path.display()))?;

    Ok(parse_ssh_config_str(&raw))
}

/// Parse SSH config content from a string.
fn parse_ssh_config_str(content: &str) -> Vec<SshConfigEntry> {
    let mut entries = Vec::new();
    let mut current: Option<SshConfigEntry> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Split into keyword and argument
        let (keyword, argument) = match trimmed.split_once(char::is_whitespace) {
            Some((k, a)) => (k.trim(), a.trim()),
            None => continue,
        };

        let keyword_lower = keyword.to_ascii_lowercase();

        if keyword_lower == "host" {
            // Save previous entry if any
            if let Some(entry) = current.take() {
                if !is_wildcard_only(&entry.alias) {
                    entries.push(entry);
                }
            }

            // Start a new entry for each host alias
            // Skip wildcard-only patterns
            let alias = argument.split_whitespace().next().unwrap_or("").to_string();
            if !alias.is_empty() {
                current = Some(SshConfigEntry {
                    alias,
                    hostname: None,
                    user: None,
                    port: None,
                    identity_file: None,
                });
            }
        } else if keyword_lower == "match" {
            // Save and skip Match blocks — they're too complex to import
            if let Some(entry) = current.take() {
                if !is_wildcard_only(&entry.alias) {
                    entries.push(entry);
                }
            }
            current = None;
        } else if let Some(ref mut entry) = current {
            match keyword_lower.as_str() {
                "hostname" => entry.hostname = Some(argument.to_string()),
                "user" => entry.user = Some(argument.to_string()),
                "port" => {
                    if let Ok(p) = argument.parse::<u16>() {
                        entry.port = Some(p);
                    }
                }
                "identityfile" => entry.identity_file = Some(argument.to_string()),
                _ => {} // ignore other directives
            }
        }
    }

    // Don't forget the last entry
    if let Some(entry) = current {
        if !is_wildcard_only(&entry.alias) {
            entries.push(entry);
        }
    }

    entries
}

fn is_wildcard_only(alias: &str) -> bool {
    alias == "*" || alias == "* " || alias.trim() == "*"
}

/// Import SSH config entries as target profiles, skipping duplicates.
/// Returns (imported, skipped) counts plus the imported profiles.
pub fn import_ssh_config_as_targets(
    existing_targets: &[TargetProfile],
) -> Result<(Vec<TargetProfile>, usize)> {
    let entries = parse_ssh_config()?;
    let mut imported = Vec::new();
    let mut skipped = 0;

    for entry in entries {
        let Some(profile) = entry.to_target_profile() else {
            skipped += 1;
            continue;
        };

        // Skip if a target with the same name already exists
        let name_exists = existing_targets
            .iter()
            .chain(imported.iter())
            .any(|t| t.name.eq_ignore_ascii_case(&profile.name));

        // Skip if a target with same host/port/user already exists
        let endpoint_exists = existing_targets.iter().chain(imported.iter()).any(|t| {
            t.host.eq_ignore_ascii_case(&profile.host)
                && t.port == profile.port
                && t.user == profile.user
        });

        if name_exists || endpoint_exists {
            skipped += 1;
            continue;
        }

        imported.push(profile);
    }

    Ok((imported, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_ssh_config() {
        let config = "\
Host myserver
    HostName 192.168.1.100
    User admin
    Port 2222
    IdentityFile ~/.ssh/id_rsa

Host devbox
    HostName dev.example.com
    User developer
";

        let entries = parse_ssh_config_str(config);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].alias, "myserver");
        assert_eq!(entries[0].hostname.as_deref(), Some("192.168.1.100"));
        assert_eq!(entries[0].user.as_deref(), Some("admin"));
        assert_eq!(entries[0].port, Some(2222));
        assert_eq!(entries[0].identity_file.as_deref(), Some("~/.ssh/id_rsa"));

        assert_eq!(entries[1].alias, "devbox");
        assert_eq!(entries[1].hostname.as_deref(), Some("dev.example.com"));
        assert_eq!(entries[1].user.as_deref(), Some("developer"));
        assert_eq!(entries[1].port, None);
    }

    #[test]
    fn skips_wildcard_hosts() {
        let config = "\
Host *
    ServerAliveInterval 60

Host myserver
    HostName 10.0.0.1
    User root
";
        let entries = parse_ssh_config_str(config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].alias, "myserver");
    }

    #[test]
    fn skips_comments_and_empty_lines() {
        let config = "\
# This is a comment
   # indented comment

Host example
    HostName example.com
    User me
";
        let entries = parse_ssh_config_str(config);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn to_target_profile_requires_user() {
        let entry = SshConfigEntry {
            alias: "nouser".to_string(),
            hostname: Some("1.2.3.4".to_string()),
            user: None,
            port: None,
            identity_file: None,
        };
        assert!(entry.to_target_profile().is_none());
    }

    #[test]
    fn to_target_profile_converts_valid_entry() {
        let entry = SshConfigEntry {
            alias: "myhost".to_string(),
            hostname: Some("10.0.0.1".to_string()),
            user: Some("admin".to_string()),
            port: Some(2222),
            identity_file: Some("~/.ssh/custom_key".to_string()),
        };

        let profile = entry.to_target_profile().expect("should convert");
        assert_eq!(profile.name, "myhost");
        assert_eq!(profile.host, "10.0.0.1");
        assert_eq!(profile.port, 2222);
        assert_eq!(profile.user, "admin");
    }

    #[test]
    fn wildcard_host_in_entry_skipped() {
        let entry = SshConfigEntry {
            alias: "wildcard".to_string(),
            hostname: Some("*.example.com".to_string()),
            user: Some("admin".to_string()),
            port: None,
            identity_file: None,
        };
        assert!(entry.to_target_profile().is_none());
    }
}
