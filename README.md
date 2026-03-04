# cyberdeck

[![Full Regression CI](https://github.com/jalsarraf0/cyberdeck/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jalsarraf0/cyberdeck/actions/workflows/ci.yml)
[![Security](https://github.com/jalsarraf0/cyberdeck/actions/workflows/security.yml/badge.svg?branch=main)](https://github.com/jalsarraf0/cyberdeck/actions/workflows/security.yml)

`cyberdeck` is a cyberpunk-themed TUI for SSH key management and remote command execution.
The app title inside the interface is `Cyber Terminal - By Snake`.

## What it does

- Lists local SSH keys in `~/.ssh`
- Generates new ed25519 keys
- Imports existing private keys (supports passphrase-protected keys)
- Stores target SSH profiles (`ip/domain`, port, user, auth)
- Exchanges a selected local public key into remote `authorized_keys`
- Fetches remote `authorized_keys`
- Runs remote SSH commands from inside the app console tab
- Supports multiple visual themes with live switching and persisted preference

## Screenshot of Cyberpunk Theme


<img width="2232" height="1293" alt="image" src="https://github.com/user-attachments/assets/1cca96c6-5c9e-45d4-b10f-8b9b457567d0" />

## TUI controls

- `1..4`: switch tabs
- `q`: quit
- Arrow keys: navigate lists
- `r`: refresh current view
- `F2`: switch theme

Per tab:

- `KEYS`: `g` generate key, `i` import existing private key
- `TARGETS`: `a` add target, `d` delete target, `t` test connection
- `EXCHANGE`: `x` exchange selected key to selected target, `f` fetch remote keys
- `SSH CONSOLE`: `e` or `Enter` to edit command, `Enter` run command, `Esc` leave edit mode, `c` clear output

Available themes:

- `Cyberpunk` (default)
- `Synth`
- `Matrix`
- `Ember`
- `Glacier`

## CLI mode

The TUI is default. Extra commands are included for automation and regression:

```bash
# list local keys
cyberdeck list-keys

# import an existing private key (optional passphrase)
cyberdeck import-key --private-key ~/.ssh/id_ed25519 --passphrase "your-passphrase"

# run command on remote host
cyberdeck run --host 10.0.0.12 --port 22 --user dev --key-file ~/.ssh/id_ed25519 --cmd "uname -a"

# exchange local public key to remote authorized_keys
cyberdeck exchange --host 10.0.0.12 --port 22 --user dev --key-file ~/.ssh/id_ed25519 --public-key ~/.ssh/id_ed25519.pub

# fetch remote authorized keys
cyberdeck fetch --host 10.0.0.12 --port 22 --user dev --key-file ~/.ssh/id_ed25519
```

For auth, use exactly one of:

- `--password <password>`
- `--key-file <private_key> [--passphrase <passphrase>]`

## Run the program

```bash
# build once
cargo build

# launch the cyberpunk TUI
cargo run
```

Optional: run the built binary directly.

```bash
./target/debug/cyberdeck
```

Install `cyberdeck` as a local command from this exact repo revision (1:1 copy):

```bash
./scripts/install-local.sh
cyberdeck
```

Install from crates.io (published package):

```bash
cargo install cyberdeck
```

Quick non-TUI checks:

```bash
cargo run -- list-keys
cargo run -- run --host 127.0.0.1 --port 22 --user your_user --key-file ~/.ssh/id_ed25519 --cmd "whoami"
```

## Security notes

- Target profiles are saved in `~/.config/cyberdeck/config.json`.
- If you choose password auth in the current implementation, passwords are stored in config for convenience.
- Regression SSH server is localhost-bound and disposable.

## Full regression test

Runs full build + integration + CLI regression against an isolated Docker SSH server and removes the container automatically:

```bash
./scripts/regression.sh
```

## Release flow

```bash
# 1) full regression
./scripts/regression.sh

# 2) verify crate package
cargo publish --dry-run

# 3) publish to crates.io
cargo publish

# 4) install local 1:1 binary used by command "cyberdeck"
./scripts/install-local.sh

# 5) publish GitHub release artifacts for Linux/macOS/Windows
git tag v$(awk -F'"' '$1 ~ /^version = / { print $2; exit }' Cargo.toml)
git push origin --tags
```

GitHub Actions workflows:

- `CI`: tests on Linux/macOS/Windows and runs full Docker regression on Linux.
- `Release`: on `v*` tags, verifies formatting/tests/regression, then builds and uploads archives to the GitHub Release page for Linux/macOS/Windows.

## Validation Status (2026-03-03)

- Regression status: PASS
- Commands validated:
  - `cargo test --all-targets`
- Result: `15 passed; 0 failed` including `tests/regression.rs`.
- CI/CD status: all tests passed on `main` (`CI` run `22642263964`, `Security` run `22642263960`).
- Security hygiene: PASS (no hardcoded secrets or private keys detected in tracked files).
