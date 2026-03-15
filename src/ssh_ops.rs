use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use ssh2::Session;

use crate::keys::{expand_home_path, read_public_key};
use crate::models::{AuthMethod, CommandResult, TargetProfile};

fn connect_session(profile: &TargetProfile) -> Result<Session> {
    let endpoint = profile.endpoint();
    let tcp = TcpStream::connect(&endpoint)
        .with_context(|| format!("failed connecting to {endpoint}"))?;
    tcp.set_read_timeout(Some(Duration::from_secs(15)))
        .with_context(|| format!("failed setting read timeout on {endpoint}"))?;
    tcp.set_write_timeout(Some(Duration::from_secs(15)))
        .with_context(|| format!("failed setting write timeout on {endpoint}"))?;

    let mut session = Session::new().context("failed creating ssh session")?;
    session.set_tcp_stream(tcp);
    session.handshake().with_context(|| {
        format!("failed SSH handshake with {}", profile.endpoint())
    })?;

    match &profile.auth {
        AuthMethod::Password { password } => {
            session
                .userauth_password(&profile.user, password)
                .with_context(|| {
                    format!("password auth failed for {}", profile.endpoint())
                })?;
        }
        AuthMethod::KeyFile {
            private_key,
            passphrase,
        } => {
            let expanded_private_key = expand_home_path(private_key)
                .context("invalid private key path")?;
            session
                .userauth_pubkey_file(
                    &profile.user,
                    None,
                    Path::new(&expanded_private_key),
                    passphrase.as_deref(),
                )
                .with_context(|| {
                    format!("key auth failed for {}", profile.endpoint())
                })?;
        }
    }

    if !session.authenticated() {
        bail!("authentication did not complete for {}", profile.endpoint());
    }

    Ok(session)
}

pub fn test_connection(profile: &TargetProfile) -> Result<()> {
    let session = connect_session(profile)?;
    if !session.authenticated() {
        return Err(anyhow!(
            "session established but authentication state is false for {}",
            profile.endpoint()
        ));
    }
    Ok(())
}

pub fn run_remote_command(profile: &TargetProfile, command: &str) -> Result<CommandResult> {
    let session = connect_session(profile)?;
    let mut channel = session
        .channel_session()
        .with_context(|| format!("failed opening channel for {}", profile.endpoint()))?;

    channel.exec(command).with_context(|| {
        format!(
            "failed executing remote command on {}: {}",
            profile.endpoint(),
            command
        )
    })?;

    let mut stdout = String::new();
    channel
        .read_to_string(&mut stdout)
        .context("failed reading command stdout")?;

    let mut stderr = String::new();
    channel
        .stderr()
        .read_to_string(&mut stderr)
        .context("failed reading command stderr")?;

    channel
        .wait_close()
        .context("failed waiting for command completion")?;
    let exit_status = channel
        .exit_status()
        .context("failed retrieving remote exit status")?;

    Ok(CommandResult {
        stdout,
        stderr,
        exit_status,
    })
}

pub fn exchange_public_key(profile: &TargetProfile, public_key_path: &Path) -> Result<()> {
    let key = read_public_key(public_key_path)?;
    let command = build_exchange_command(&key);

    let result = run_remote_command(profile, &command)?;
    if result.exit_status != 0 {
        return Err(anyhow!(
            "failed exchanging key on {} (exit={}): {}",
            profile.endpoint(),
            result.exit_status,
            result.stderr.trim()
        ));
    }

    Ok(())
}

pub fn fetch_remote_authorized_keys(profile: &TargetProfile) -> Result<Vec<String>> {
    let result = run_remote_command(profile, fetch_authorized_keys_command())?;

    if result.exit_status != 0 {
        return Err(anyhow!(
            "failed reading remote authorized_keys on {} (exit={}): {}",
            profile.endpoint(),
            result.exit_status,
            result.stderr.trim()
        ));
    }

    let mut keys = Vec::new();
    for line in result.stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            keys.push(trimmed.to_string());
        }
    }
    Ok(keys)
}

fn build_exchange_command(public_key: &str) -> String {
    let escaped = shell_escape_single(public_key);
    format!(
        "umask 077; mkdir -p ~/.ssh; chmod 700 ~/.ssh; touch ~/.ssh/authorized_keys; chmod 600 ~/.ssh/authorized_keys; \
         grep -qxF {escaped} ~/.ssh/authorized_keys || echo {escaped} >> ~/.ssh/authorized_keys"
    )
}

fn fetch_authorized_keys_command() -> &'static str {
    "if [ -f ~/.ssh/authorized_keys ]; then cat ~/.ssh/authorized_keys; fi"
}

fn shell_escape_single(input: &str) -> String {
    if input.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{build_exchange_command, fetch_authorized_keys_command, shell_escape_single};

    #[test]
    fn shell_escape_handles_quotes() {
        let escaped = shell_escape_single("abc'def");
        assert_eq!(escaped, "'abc'\"'\"'def'");
    }

    #[test]
    fn exchange_command_enforces_permissions_and_idempotency() {
        let command = build_exchange_command("ssh-ed25519 AAAA test@example");
        assert!(command.contains("chmod 700 ~/.ssh"));
        assert!(command.contains("chmod 600 ~/.ssh/authorized_keys"));
        assert!(command.contains("grep -qxF"));
    }

    #[test]
    fn fetch_authorized_keys_command_does_not_mask_errors() {
        let command = fetch_authorized_keys_command();
        assert!(!command.contains("|| true"));
    }
}
