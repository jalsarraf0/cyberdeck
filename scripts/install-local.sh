#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ROOT_DIR="$HOME/.local"
EXPECTED_VERSION="$(awk -F'"' '$1 ~ /^version = / { print $2; exit }' Cargo.toml)"

echo "Installing cyberdeck from local repository path..."
cargo install --path . --locked --force --root "$ROOT_DIR"

RESOLVED_BIN="$(command -v cyberdeck || true)"
if [[ -z "$RESOLVED_BIN" ]]; then
  echo "cyberdeck is not on PATH after install"
  exit 1
fi

VERSION_OUTPUT="$(cyberdeck --version)"
if [[ "$VERSION_OUTPUT" != *"$EXPECTED_VERSION"* ]]; then
  echo "cyberdeck version mismatch. Expected $EXPECTED_VERSION, got: $VERSION_OUTPUT"
  echo "Resolved binary: $RESOLVED_BIN"
  exit 1
fi

echo "Installed binary: $RESOLVED_BIN"
echo "$VERSION_OUTPUT"
