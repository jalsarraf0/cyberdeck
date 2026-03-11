# CLAUDE.md — cyberdeck

Rust TUI + CLI tool for SSH key management: generate keys, add targets, exchange
public keys with remote hosts, and run SSH commands. Built with Ratatui.

Config: `~/.config/cyberdeck/config.json`

Requires: `rust-edition = "2024"`, `rust-version = "1.85"`.

---

## Build and Run

```bash
cargo build                    # debug build
cargo run                      # launch TUI
cargo run -- list-keys         # CLI: list keys
cargo run -- run --host <ip> --port 22 --user <user> \
  --key-file ~/.ssh/id_ed25519 --cmd "whoami"
```

---

## Development Commands

```bash
cargo test --all-targets       # all tests
cargo fmt --all --check        # format check (CI gate)
cargo clippy --workspace --all-targets -- -D warnings  # lint (CI gate)
./scripts/regression.sh        # full Docker-based regression suite
./scripts/install-local.sh     # install binary to local PATH
```

---

## CI Gate (must be clean before commit/push)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --all-targets
```

---

## Source Layout

```
src/
  main.rs       # Entry point (TUI/CLI dispatch)
  lib.rs        # Library re-exports
  models.rs     # Core data models (Key, Target, etc.)
  keys.rs       # Key generation and management
  ssh_ops.rs    # SSH operations (exchange, remote command)
  storage.rs    # Config JSON persistence
  tui.rs        # Ratatui TUI implementation
scripts/
  regression.sh      # Docker-based regression runner
  install-local.sh   # Local binary install helper
tests/               # Integration tests
```

---

## TUI Keybindings

`1-4` switch tabs, `q` quit, `F2` theme, `g` generate key, `a` add target,
`x` exchange key, `e` / Enter run SSH command.

---

## Conventions

- `anyhow::Result` for errors. No `.unwrap()` in library code.
- All SSH operations in `ssh_ops.rs`. Do not shell out to `ssh` binary — use the
  library crate.
- Config is JSON (`~/.config/cyberdeck/config.json`). Do not migrate to a different format.
- TUI state lives in `tui.rs`. Keep it separate from business logic.
- `rust-version = "1.85"` — do not use nightly-only features.

---

## Validation

```bash
cargo test --all-targets
cargo clippy --workspace --all-targets -- -D warnings
./scripts/regression.sh
```

---

## Toolchain

| Tool | Path | Version |
|---|---|---|
| rustc | `/usr/bin/rustc` | 1.93.1 (Fedora dnf) |
| cargo | `/usr/bin/cargo` | 1.93.1 (Fedora dnf) |
| rustfmt | `/usr/bin/rustfmt` | 1.93.1 |
| rust-analyzer | `/usr/bin/rust-analyzer` | 1.93.1 |

Rust is system-installed via dnf, not rustup shims. `rustup` is present at `~/.rustup` but
its shims are not active — `/usr/bin/rustc` takes priority.
`~/.cargo/bin/` is in PATH for user-installed cargo tools (cargo-audit, cargo-deny, aihelp, cyberdeck).
