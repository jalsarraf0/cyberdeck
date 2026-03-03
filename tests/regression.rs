use std::env;
use std::path::Path;
use std::process::Command;

use keyex::models::{AuthMethod, TargetProfile};
use keyex::ssh_ops::{
    exchange_public_key, fetch_remote_authorized_keys, run_remote_command, test_connection,
};
use tempfile::TempDir;

#[test]
fn regression_ssh_exchange_and_command_flow() {
    let Some(host) = env::var("KEYEX_TEST_HOST").ok() else {
        eprintln!("SKIP: KEYEX_TEST_HOST is not set");
        return;
    };
    let Some(port_raw) = env::var("KEYEX_TEST_PORT").ok() else {
        eprintln!("SKIP: KEYEX_TEST_PORT is not set");
        return;
    };
    let Some(user) = env::var("KEYEX_TEST_USER").ok() else {
        eprintln!("SKIP: KEYEX_TEST_USER is not set");
        return;
    };
    let Some(private_key) = env::var("KEYEX_TEST_KEY").ok() else {
        eprintln!("SKIP: KEYEX_TEST_KEY is not set");
        return;
    };

    let port = port_raw
        .parse::<u16>()
        .expect("KEYEX_TEST_PORT must be u16");

    let profile = TargetProfile {
        name: "regression-target".to_string(),
        host,
        port,
        user,
        auth: AuthMethod::KeyFile {
            private_key,
            passphrase: None,
        },
    };

    test_connection(&profile).expect("connection test failed");

    let result = run_remote_command(&profile, "echo keyex_regression_ok")
        .expect("running remote command failed");
    assert_eq!(result.exit_status, 0, "unexpected exit status");
    assert!(
        result.stdout.contains("keyex_regression_ok"),
        "stdout did not contain expected marker: {}",
        result.stdout
    );

    let temp = TempDir::new().expect("tempdir");
    let key_base = temp.path().join("exchange_key");
    let status = Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            "keyex-regression@local",
            "-f",
            key_base.to_str().unwrap(),
        ])
        .status()
        .expect("ssh-keygen spawn failed");
    assert!(status.success(), "ssh-keygen failed");

    let pub_path = key_base.with_extension("pub");
    exchange_public_key(&profile, Path::new(&pub_path)).expect("key exchange failed");

    let remote_keys = fetch_remote_authorized_keys(&profile).expect("fetch remote keys failed");
    let expected = std::fs::read_to_string(&pub_path)
        .expect("read generated public key")
        .trim()
        .to_string();

    assert!(
        remote_keys.iter().any(|line| line.trim() == expected),
        "expected exchanged key not found on remote authorized_keys"
    );
}
