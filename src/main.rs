use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use keyex::keys::{expand_home_path, import_private_key, scan_local_keys};
use keyex::models::{AuthMethod, TargetProfile};
use keyex::ssh_ops::{exchange_public_key, fetch_remote_authorized_keys, run_remote_command};
use keyex::tui::run_tui;

#[derive(Parser, Debug)]
#[command(name = "keyex")]
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
    use keyex::models::AuthMethod;

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
