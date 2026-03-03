#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "Installing cyberdeck from local repository path..."
cargo install --path . --locked --force

echo "Installed binary: $(command -v cyberdeck)"
cyberdeck --version
