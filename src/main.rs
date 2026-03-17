use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use cyberdeck::health::{Severity, audit_keys};
use cyberdeck::keys::{expand_home_path, import_private_key, scan_local_keys};
use cyberdeck::models::{AuthMethod, TargetProfile};
use cyberdeck::ssh_config::import_ssh_config_as_targets;
use cyberdeck::ssh_ops::{exchange_public_key, fetch_remote_authorized_keys, run_remote_command};
use cyberdeck::storage::{load_config, save_config};
use cyberdeck::tui::run_tui;

#[derive(Parser, Debug)]
#[command(name = "cyberdeck")]
#[command(version)]
#[command(about = "Cyberpunk SSH key exchange and command console")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the interactive TUI (default)
    Tui,
    /// List local SSH keys discovered in ~/.ssh
    ListKeys,
    /// Import an existing private key (optionally passphrase-protected) into ~/.ssh/*.pub catalog
    ImportKey {
        #[arg(long)]
        private_key: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Exchange (install) a local public key to a remote authorized_keys file
    Exchange {
        #[arg(long)]
        host: String,
        #[arg(long, default_value_t = 22)]
        port: u16,
        #[arg(long)]
        user: String,
        #[arg(long)]
        public_key: String,
        #[arg(long, conflicts_with = "key_file")]
        password: Option<String>,
        #[arg(long, conflicts_with = "password")]
        key_file: Option<String>,
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Run a command on the remote target over SSH
    Run {
        #[arg(long)]
        host: String,
        #[arg(long, default_value_t = 22)]
        port: u16,
        #[arg(long)]
        user: String,
        #[arg(long)]
        cmd: String,
        #[arg(long, conflicts_with = "key_file")]
        password: Option<String>,
        #[arg(long, conflicts_with = "password")]
        key_file: Option<String>,
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Fetch and print remote authorized_keys entries
    Fetch {
        #[arg(long)]
        host: String,
        #[arg(long, default_value_t = 22)]
        port: u16,
        #[arg(long)]
        user: String,
        #[arg(long, conflicts_with = "key_file")]
        password: Option<String>,
        #[arg(long, conflicts_with = "password")]
        key_file: Option<String>,
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Import SSH targets from ~/.ssh/config (skips duplicates)
    ImportConfig,
    /// Audit local SSH keys for security issues
    AuditKeys,
    /// Export saved targets as runnable SSH commands
    Export,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => run_tui(),
        Some(Commands::ListKeys) => cmd_list_keys(),
        Some(Commands::ImportKey {
            private_key,
            name,
            passphrase,
        }) => {
            let imported =
                import_private_key(name.as_deref(), &private_key, passphrase.as_deref())?;
            println!("Imported {} -> {}", imported.name, imported.public_key_path);
            Ok(())
        }
        Some(Commands::Exchange {
            host,
            port,
            user,
            public_key,
            password,
            key_file,
            passphrase,
        }) => {
            let profile = build_profile(host, port, user, password, key_file, passphrase)?;
            let public_key_path = expand_home_path(&public_key)?;
            exchange_public_key(&profile, &public_key_path)?;
            println!("Key exchange complete: {}", profile.endpoint());
            Ok(())
        }
        Some(Commands::Run {
            host,
            port,
            user,
            cmd,
            password,
            key_file,
            passphrase,
        }) => {
            let profile = build_profile(host, port, user, password, key_file, passphrase)?;
            let result = run_remote_command(&profile, &cmd)?;
            if !result.stdout.is_empty() {
                print!("{}", result.stdout);
            }
            if !result.stderr.is_empty() {
                eprint!("{}", result.stderr);
            }
            println!("\n[exit {}]", result.exit_status);
            Ok(())
        }
        Some(Commands::Fetch {
            host,
            port,
            user,
            password,
            key_file,
            passphrase,
        }) => {
            let profile = build_profile(host, port, user, password, key_file, passphrase)?;
            let keys = fetch_remote_authorized_keys(&profile)?;
            for (idx, key) in keys.iter().enumerate() {
                println!("{}: {}", idx + 1, key);
            }
            println!("Total keys: {}", keys.len());
            Ok(())
        }
        Some(Commands::ImportConfig) => cmd_import_config(),
        Some(Commands::AuditKeys) => cmd_audit_keys(),
        Some(Commands::Export) => cmd_export(),
    }
}

fn cmd_list_keys() -> Result<()> {
    let keys = scan_local_keys()?;
    if keys.is_empty() {
        println!("No local keys found in ~/.ssh");
        return Ok(());
    }

    for key in keys {
        println!("{} | {} | {}", key.name, key.algorithm, key.public_key_path);
    }
    Ok(())
}

fn cmd_import_config() -> Result<()> {
    let mut config = load_config()?;
    let (imported, skipped) = import_ssh_config_as_targets(&config.targets)?;

    if imported.is_empty() {
        println!(
            "No new targets to import from ~/.ssh/config ({} skipped as duplicates).",
            skipped
        );
        return Ok(());
    }

    let count = imported.len();
    for profile in &imported {
        println!(
            "  + {} -> {}@{}:{}",
            profile.name, profile.user, profile.host, profile.port
        );
    }
    config.targets.extend(imported);
    save_config(&config)?;
    println!(
        "Imported {} target(s), skipped {} duplicate(s).",
        count, skipped
    );
    Ok(())
}

fn cmd_audit_keys() -> Result<()> {
    let keys = scan_local_keys()?;
    let findings = audit_keys(&keys);

    if findings.is_empty() {
        println!("No findings. All keys look good.");
        return Ok(());
    }

    let mut criticals = 0;
    let mut warns = 0;

    for finding in &findings {
        match finding.severity {
            Severity::Critical => {
                criticals += 1;
                eprintln!("{finding}");
            }
            Severity::Warn => {
                warns += 1;
                eprintln!("{finding}");
            }
            _ => println!("{finding}"),
        }
    }

    println!(
        "\nSummary: {} finding(s) — {} critical, {} warning(s)",
        findings.len(),
        criticals,
        warns
    );
    Ok(())
}

fn cmd_export() -> Result<()> {
    let config = load_config()?;
    if config.targets.is_empty() {
        println!("No targets configured. Use 'cyberdeck import-config' or add targets in the TUI.");
        return Ok(());
    }

    for target in &config.targets {
        println!("# {}", target.name);
        println!("{}", format_ssh_command(target));
        println!();
    }
    Ok(())
}

/// Format a target profile as a runnable SSH command.
///
/// Key-file auth → `ssh -i <key> [-p port] user@host`
/// Password auth → comment noting sshpass or manual entry is needed
fn format_ssh_command(profile: &TargetProfile) -> String {
    // TODO: This is a great place for you to implement!
    // Consider: key-file vs password auth, non-standard ports, passphrase handling.
    // See the guidance in the conversation above.
    let mut parts = vec!["ssh".to_string()];

    match &profile.auth {
        AuthMethod::KeyFile { private_key, .. } => {
            parts.push("-i".to_string());
            parts.push(private_key.clone());
        }
        AuthMethod::Password { .. } => {
            // Password auth can't be expressed safely in a plain ssh command.
            // Emit as a comment so the user knows to enter it interactively.
            parts.insert(
                0,
                "# (password auth — enter password when prompted)\n".to_string(),
            );
        }
    }

    if profile.port != 22 {
        parts.push("-p".to_string());
        parts.push(profile.port.to_string());
    }

    parts.push(format!("{}@{}", profile.user, profile.host));
    parts.join(" ")
}

fn build_profile(
    host: String,
    port: u16,
    user: String,
    password: Option<String>,
    key_file: Option<String>,
    passphrase: Option<String>,
) -> Result<TargetProfile> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err(anyhow!("host cannot be empty"));
    }
    let user = user.trim().to_string();
    if user.is_empty() {
        return Err(anyhow!("user cannot be empty"));
    }

    let auth = match (password, key_file) {
        (Some(password), None) => {
            if password.is_empty() {
                return Err(anyhow!("password cannot be empty"));
            }
            AuthMethod::Password { password }
        }
        (None, Some(private_key)) => {
            let private_key = private_key.trim().to_string();
            if private_key.is_empty() {
                return Err(anyhow!("key-file cannot be empty"));
            }
            AuthMethod::KeyFile {
                private_key,
                passphrase: passphrase.filter(|p| !p.is_empty()),
            }
        }
        _ => {
            return Err(anyhow!(
                "authentication required: use exactly one of --password or --key-file"
            ));
        }
    };

    Ok(TargetProfile {
        name: format!("{}@{}:{}", user, host, port),
        host,
        port,
        user,
        auth,
    })
}

#[cfg(test)]
mod tests {
    use super::build_profile;
    use cyberdeck::models::{AuthMethod, TargetProfile};

    #[test]
    fn build_profile_rejects_empty_host_and_user() {
        assert!(
            build_profile(
                " ".to_string(),
                22,
                "user".to_string(),
                Some("pw".to_string()),
                None,
                None
            )
            .is_err()
        );

        assert!(
            build_profile(
                "127.0.0.1".to_string(),
                22,
                " ".to_string(),
                Some("pw".to_string()),
                None,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn format_ssh_command_keyfile_standard_port() {
        let profile = TargetProfile {
            name: "dev".to_string(),
            host: "10.0.0.1".to_string(),
            port: 22,
            user: "deploy".to_string(),
            auth: AuthMethod::KeyFile {
                private_key: "~/.ssh/id_ed25519".to_string(),
                passphrase: None,
            },
        };
        let cmd = super::format_ssh_command(&profile);
        assert_eq!(cmd, "ssh -i ~/.ssh/id_ed25519 deploy@10.0.0.1");
    }

    #[test]
    fn format_ssh_command_keyfile_custom_port() {
        let profile = TargetProfile {
            name: "prod".to_string(),
            host: "prod.example.com".to_string(),
            port: 2222,
            user: "admin".to_string(),
            auth: AuthMethod::KeyFile {
                private_key: "~/.ssh/prod_key".to_string(),
                passphrase: None,
            },
        };
        let cmd = super::format_ssh_command(&profile);
        assert_eq!(cmd, "ssh -i ~/.ssh/prod_key -p 2222 admin@prod.example.com");
    }

    #[test]
    fn format_ssh_command_password_auth() {
        let profile = TargetProfile {
            name: "legacy".to_string(),
            host: "old.server".to_string(),
            port: 22,
            user: "root".to_string(),
            auth: AuthMethod::Password {
                password: "secret".to_string(),
            },
        };
        let cmd = super::format_ssh_command(&profile);
        assert!(cmd.contains("password auth"));
        assert!(cmd.contains("ssh"));
        assert!(cmd.contains("root@old.server"));
    }

    #[test]
    fn build_profile_trims_key_file_and_empty_passphrase() {
        let profile = build_profile(
            "127.0.0.1".to_string(),
            22,
            "tester".to_string(),
            None,
            Some(" ~/.ssh/id_ed25519 ".to_string()),
            Some("".to_string()),
        )
        .expect("profile");

        match profile.auth {
            AuthMethod::KeyFile {
                private_key,
                passphrase,
            } => {
                assert_eq!(private_key, "~/.ssh/id_ed25519");
                assert!(passphrase.is_none());
            }
            AuthMethod::Password { .. } => panic!("expected key auth"),
        }
    }
}
