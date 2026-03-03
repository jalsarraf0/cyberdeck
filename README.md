# cyberdeck

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
