#!/usr/bin/env bash
set -euo pipefail

IMAGE="lscr.io/linuxserver/openssh-server:latest"
CONTAINER_NAME="cyberdeck-regression-$(date +%s)-$RANDOM"
WORKDIR="$(mktemp -d)"
CLIENT_KEY_BASE="$WORKDIR/client_regression_key"
EXCHANGE_KEY_BASE="$WORKDIR/exchange_key"
PASS_KEY_BASE="$WORKDIR/passphrase_key"
PASS_KEY_PASSPHRASE="cyberdeck-passphrase-regression"
KEY_IMPORT_NAME="cyberdeck_regression_imported_$RANDOM"
IMPORTED_PUB="$HOME/.ssh/${KEY_IMPORT_NAME}.pub"

cleanup() {
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
  rm -f "$IMPORTED_PUB"
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

if ! docker info >/dev/null 2>&1; then
  echo "Skipping Docker-backed regression: docker daemon is unavailable or inaccessible."
  exit 0
fi

ssh-keygen -q -t ed25519 -N "" -C "cyberdeck-regression-client" -f "$CLIENT_KEY_BASE" >/dev/null
CLIENT_PUBLIC_KEY="$(cat "$CLIENT_KEY_BASE.pub")"

# Isolated SSH server: localhost-bound and disposable.
docker run -d --rm \
  --name "$CONTAINER_NAME" \
  -p 127.0.0.1::2222 \
  -e PUID="$(id -u)" \
  -e PGID="$(id -g)" \
  -e TZ="UTC" \
  -e USER_NAME="tester" \
  -e PUBLIC_KEY="$CLIENT_PUBLIC_KEY" \
  -e PASSWORD_ACCESS="false" \
  -e SUDO_ACCESS="false" \
  "$IMAGE" >/dev/null

PORT_LINE="$(docker port "$CONTAINER_NAME" 2222/tcp)"
PORT="${PORT_LINE##*:}"

echo "Waiting for SSH server on 127.0.0.1:$PORT ..."
for _ in $(seq 1 90); do
  if ssh -o BatchMode=yes \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
      -i "$CLIENT_KEY_BASE" \
      -p "$PORT" tester@127.0.0.1 "echo ready" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

ssh -o BatchMode=yes \
  -o StrictHostKeyChecking=no \
  -o UserKnownHostsFile=/dev/null \
  -i "$CLIENT_KEY_BASE" \
  -p "$PORT" tester@127.0.0.1 "echo ready" >/dev/null

echo "Building project..."
cargo build

echo "Running Rust integration regression test..."
CYBERDECK_TEST_HOST=127.0.0.1 \
CYBERDECK_TEST_PORT="$PORT" \
CYBERDECK_TEST_USER=tester \
CYBERDECK_TEST_KEY="$CLIENT_KEY_BASE" \
cargo test --test regression -- --nocapture

echo "Running CLI regression checks..."
cargo run --quiet -- list-keys >/dev/null

cargo run --quiet -- import-key \
  --private-key "$CLIENT_KEY_BASE" \
  --name "$KEY_IMPORT_NAME" >/dev/null

if [[ ! -f "$IMPORTED_PUB" ]]; then
  echo "CLI import-key regression failed: imported public key file missing"
  exit 1
fi

RUN_OUTPUT="$(cargo run --quiet -- run \
  --host 127.0.0.1 \
  --port "$PORT" \
  --user tester \
  --key-file "$CLIENT_KEY_BASE" \
  --cmd "echo cli_regression_ok")"

if [[ "$RUN_OUTPUT" != *"cli_regression_ok"* ]]; then
  echo "CLI run regression failed: expected marker not found"
  exit 1
fi

ssh-keygen -q -t ed25519 -N "" -C "cyberdeck-regression-exchange" -f "$EXCHANGE_KEY_BASE" >/dev/null
cargo run --quiet -- exchange \
  --host 127.0.0.1 \
  --port "$PORT" \
  --user tester \
  --key-file "$CLIENT_KEY_BASE" \
  --public-key "$EXCHANGE_KEY_BASE.pub" >/dev/null

FETCH_OUTPUT="$(cargo run --quiet -- fetch \
  --host 127.0.0.1 \
  --port "$PORT" \
  --user tester \
  --key-file "$CLIENT_KEY_BASE")"

EXCHANGE_LINE="$(cat "$EXCHANGE_KEY_BASE.pub")"
if [[ "$FETCH_OUTPUT" != *"$EXCHANGE_LINE"* ]]; then
  echo "CLI fetch regression failed: exchanged key missing"
  exit 1
fi

ssh-keygen -q -t ed25519 -N "$PASS_KEY_PASSPHRASE" -C "cyberdeck-regression-passphrase" -f "$PASS_KEY_BASE" >/dev/null
cargo run --quiet -- exchange \
  --host 127.0.0.1 \
  --port "$PORT" \
  --user tester \
  --key-file "$CLIENT_KEY_BASE" \
  --public-key "$PASS_KEY_BASE.pub" >/dev/null

PASS_OUTPUT="$(cargo run --quiet -- run \
  --host 127.0.0.1 \
  --port "$PORT" \
  --user tester \
  --key-file "$PASS_KEY_BASE" \
  --passphrase "$PASS_KEY_PASSPHRASE" \
  --cmd "echo passphrase_regression_ok")"

if [[ "$PASS_OUTPUT" != *"passphrase_regression_ok"* ]]; then
  echo "Passphrase key regression failed: expected marker not found"
  exit 1
fi

echo "Regression complete. Container teardown confirmed by EXIT trap."
