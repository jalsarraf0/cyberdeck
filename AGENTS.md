# cyberdeck AGENTS

## What This Repo Does

`cyberdeck` is a Rust Ratatui TUI and CLI for SSH key management, target profiles, key exchange, and remote command execution. Runtime config is stored at `~/.config/cyberdeck/config.json`.

## Main Entrypoints

- `src/main.rs`: CLI and TUI entrypoint.
- `src/tui.rs`: TUI implementation.
- `src/ssh_ops.rs`: SSH actions.
- `src/storage.rs`: config persistence.
- `scripts/regression.sh`: Docker-backed regression suite.
- `scripts/install-local.sh`: local install helper.

## Commands

- `cargo run`
- `cargo run -- list-keys`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --all-targets`
- `./scripts/regression.sh`

## Repo-Specific Constraints

- Keep SSH behavior in the library code; do not shell out to the `ssh` binary.
- Keep config as JSON at the existing path.
- Separate TUI state from business logic.
- Stay compatible with stable Rust and the repo's declared toolchain requirements.

## Agent Notes

- Validate Cargo targets for code changes and run the regression script when SSH behavior changes.
- Avoid unrelated TUI churn when changing lower-level SSH logic.
