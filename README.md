# cyberdeck

[![CI](https://github.com/jalsarraf0/cyberdeck/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jalsarraf0/cyberdeck/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> CI runs on self-hosted runners managed by [haskell-ci-orchestrator](https://github.com/jalsarraf0/haskell-ci-orchestrator) with build attestation.

A cyberpunk-themed terminal UI for SSH key management, key exchange, and remote command execution. Built with Rust and [Ratatui](https://ratatui.rs).

---

## Demo

<img width="2227" height="1288" alt="Cyberdeck TUI — Cyberpunk theme" src="https://github.com/user-attachments/assets/a37a1ea7-2bf5-498e-8fb0-05b448901f19" />

---

## Features

- **Key management** — list, generate (ed25519), and import SSH keys from `~/.ssh`
- **Target profiles** — store SSH connection profiles (host, port, user, auth method)
- **Key exchange** — deploy a local public key to a remote host's `authorized_keys`
- **Remote commands** — run commands on targets from the built-in SSH console
- **SSH config import** — pull targets from `~/.ssh/config` automatically
- **Key health audit** — detect weak algorithms, old keys, and orphaned public keys
- **Export** — print saved targets as ready-to-run `ssh` commands for scripting
- **Host key verification** — checks `~/.ssh/known_hosts` to prevent MITM attacks
- **Credential safety** — passwords and passphrases are never written to disk
- **Themes** — five built-in themes (Cyberpunk, Synth, Matrix, Ember, Glacier) with live switching

---

## Quick start

### Install from a release binary

Download the latest binary for your platform from the [Releases](https://github.com/jalsarraf0/cyberdeck/releases/latest) page.

| Platform | Asset |
|---|---|
| Linux x86_64 | `cyberdeck-*-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `cyberdeck-*-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel | `cyberdeck-*-x86_64-apple-darwin.tar.gz` |
| macOS Apple Silicon | `cyberdeck-*-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `cyberdeck-*-x86_64-pc-windows-msvc.zip` |

Linux packages are also available: `.rpm`, `.deb`, and `.pkg.tar.zst` (Arch/Pacman).

### Install from source

```bash
cargo install cyberdeck
```

Or build from this repo:

```bash
git clone https://github.com/jalsarraf0/cyberdeck.git
cd cyberdeck
cargo build --release
./target/release/cyberdeck
```

### Install locally (development)

```bash
./scripts/install-local.sh
cyberdeck
```

---

## Usage

Launch the TUI (default):

```bash
cyberdeck
```

### TUI keybindings

| Key | Action |
|---|---|
| `1`..`4` | Switch tabs (Keys, Targets, Exchange, Console) |
| `q` | Quit |
| Arrow keys | Navigate lists |
| `r` | Refresh current view |
| `F2` | Cycle theme |

**Keys tab:** `g` generate key, `i` import private key
**Targets tab:** `a` add target, `d` delete target, `t` test connection
**Exchange tab:** `x` exchange key to target, `f` fetch remote keys
**Console tab:** `e`/`Enter` edit command, `Enter` run, `Esc` leave edit, `c` clear

### CLI commands

```bash
# List local SSH keys
cyberdeck list-keys

# Import a private key (optional passphrase)
cyberdeck import-key --private-key ~/.ssh/id_ed25519

# Import targets from ~/.ssh/config
cyberdeck import-config

# Audit keys for security issues
cyberdeck audit-keys

# Export saved targets as ssh commands
cyberdeck export

# Run a remote command
cyberdeck run --host 10.0.0.12 --port 22 --user dev \
  --key-file ~/.ssh/id_ed25519 --cmd "uname -a"

# Exchange a public key to remote authorized_keys
cyberdeck exchange --host 10.0.0.12 --port 22 --user dev \
  --key-file ~/.ssh/id_ed25519 --public-key ~/.ssh/id_ed25519.pub

# Fetch remote authorized_keys
cyberdeck fetch --host 10.0.0.12 --port 22 --user dev \
  --key-file ~/.ssh/id_ed25519
```

For SSH authentication, use exactly one of:

- `--key-file <path> [--passphrase <passphrase>]`
- `--password <password>`

---

## Security

- **Host key verification** — remote host keys are checked against `~/.ssh/known_hosts` on every connection. A mismatch aborts with a clear MITM warning.
- **No secrets on disk** — passwords and passphrases are stripped before the config file is written. They exist only in memory for the current session.
- **Config permissions** — `~/.config/cyberdeck/config.json` is created with `0600` (owner-only) permissions; the directory with `0700`.
- **Key name validation** — rejects path separators and null bytes to prevent path traversal.
- **No unsafe code** — the crate uses `#![forbid(unsafe_code)]`.

---

## Development

### CI gate (run before committing)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --all-targets
```

### Full regression test

Runs a Docker-backed SSH server, exercises all CLI flows, and cleans up automatically:

```bash
./scripts/regression.sh
```

### Haskell CI orchestrator

The `ci/orchestrator/` Haskell tool generates all GitHub Actions workflows and packaging specs. Use it instead of editing YAML by hand:

```bash
cabal run orchestrator -- ci        # run local CI gate
cabal run orchestrator -- generate  # regenerate workflows
cabal run orchestrator -- release   # regenerate packaging files
cabal run orchestrator -- full      # ci + generate + release
```

---

## Release flow

```bash
# 1. Run the orchestrator full pipeline
cabal run orchestrator -- full

# 2. Commit and push
git add -A && git commit
git push origin main

# 3. Tag and push to trigger the release workflow
git tag -a v$(awk -F'"' '$1 ~ /^version = / {print $2; exit}' Cargo.toml) -m "Release"
git push origin --tags
```

The release workflow automatically:

1. Verifies the tag matches `Cargo.toml` version
2. Runs the full CI gate + Docker regression
3. Builds binaries for 5 targets (Linux x86_64/ARM64, macOS Intel/Silicon, Windows)
4. Packages Linux binaries as RPM, DEB, and Pacman
5. Publishes a GitHub Release with all assets and build provenance attestation

---

## Project structure

```
src/
  main.rs        Entry point (TUI/CLI dispatch)
  lib.rs         Library re-exports
  models.rs      Core data models (Key, Target, etc.)
  keys.rs        Key generation and management
  ssh_ops.rs     SSH operations (exchange, remote command, host key verification)
  storage.rs     Config JSON persistence (with credential sanitization)
  tui.rs         Ratatui TUI implementation
  health.rs      Key health auditing
  ssh_config.rs  SSH config parsing and import
ci/orchestrator/ Haskell CI/CD orchestrator
packaging/       Generated RPM/DEB/Pacman specs
scripts/         Regression and install helpers
```

---

## License

[Apache-2.0](LICENSE)
