#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
trunk build --release --public-url /

echo "Build complete: $ROOT_DIR/dist"
