use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::models::AppConfig;

const CONFIG_DIR_OVERRIDE_ENV: &str = "KEYEX_CONFIG_DIR";

pub fn config_dir() -> Result<PathBuf> {
    resolve_config_dir(
        dirs::config_dir(),
        std::env::var_os(CONFIG_DIR_OVERRIDE_ENV),
    )
}

pub fn config_path() -> Result<PathBuf> {
    let mut path = config_dir()?;
    path.push("config.json");
    Ok(path)
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    load_config_from_path(&path)
}

pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let path = config_path()?;
    save_config_to_path(cfg, &path)
}

fn resolve_config_dir(
    default_config_dir: Option<PathBuf>,
    override_dir: Option<OsString>,
) -> Result<PathBuf> {
    if let Some(override_dir) = override_dir {
        let override_path = PathBuf::from(&override_dir);
        if override_path.as_os_str().is_empty() {
            return Err(anyhow::anyhow!(
                "{CONFIG_DIR_OVERRIDE_ENV} is set but empty"
            ));
        }
        return Ok(override_path);
    }

    let mut path = default_config_dir.context("could not locate config directory")?;
    path.push("keyex");
    Ok(path)
}

fn load_config_from_path(path: &Path) -> Result<AppConfig> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed reading config file: {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(AppConfig::default());
    }

    let cfg = serde_json::from_str::<AppConfig>(&raw)
        .with_context(|| format!("failed parsing config file: {}", path.display()))?;
    Ok(cfg)
}

fn save_config_to_path(cfg: &AppConfig, path: &Path) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config path has no parent: {}", path.display()))?;
    fs::create_dir_all(dir)
        .with_context(|| format!("failed creating config dir: {}", dir.display()))?;

    #[cfg(unix)]
    set_dir_permissions(dir)?;

    let raw = serde_json::to_string_pretty(cfg).context("failed serializing config")?;
    let tmp_path = dir.join(format!(".config.{}.tmp", std::process::id()));
    fs::write(&tmp_path, raw)
        .with_context(|| format!("failed writing temp config file: {}", tmp_path.display()))?;

    #[cfg(unix)]
    set_file_permissions(&tmp_path)?;

    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "failed replacing config file {} with {}",
                path.display(),
                tmp_path.display()
            )
        });
    }

    #[cfg(unix)]
    set_file_permissions(path)?;

    Ok(())
}

#[cfg(unix)]
fn set_dir_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).with_context(|| {
        format!(
            "failed setting secure permissions on dir: {}",
            dir.display()
        )
    })
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).with_context(|| {
        format!(
            "failed setting secure permissions on file: {}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use tempfile::TempDir;

    use super::{load_config_from_path, resolve_config_dir, save_config_to_path};
    use crate::models::{AppConfig, AuthMethod, TargetProfile};

    #[test]
    fn resolve_config_dir_uses_override_when_present() {
        let override_dir = OsString::from("/tmp/keyex-test-override");
        let resolved =
            resolve_config_dir(None, Some(override_dir.clone())).expect("resolve override");
        assert_eq!(resolved, std::path::PathBuf::from(override_dir));
    }

    #[test]
    fn load_empty_config_file_returns_default() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("config.json");
        fs::write(&path, "\n  \n").expect("write empty file");

        let cfg = load_config_from_path(&path).expect("load config");
        assert!(cfg.targets.is_empty());
        assert!(cfg.theme.is_none());
    }

    #[test]
    fn save_then_load_round_trip() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("config.json");

        let cfg = AppConfig {
            targets: vec![TargetProfile {
                name: "local".to_string(),
                host: "127.0.0.1".to_string(),
                port: 2222,
                user: "tester".to_string(),
                auth: AuthMethod::Password {
                    password: "secret".to_string(),
                },
            }],
            theme: Some("matrix".to_string()),
        };

        save_config_to_path(&cfg, &path).expect("save config");
        let loaded = load_config_from_path(&path).expect("load config");

        assert_eq!(loaded.targets.len(), 1);
        assert_eq!(loaded.targets[0].name, "local");
        assert_eq!(loaded.targets[0].endpoint(), "127.0.0.1:2222");
        assert_eq!(loaded.theme.as_deref(), Some("matrix"));
    }
}
