#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(dirname "$0")/../../../.."
EXT_DIR="$ROOT_DIR/packages/lsp/vscode"
BIN_DIR="$EXT_DIR/server/linux-x64"

cargo build -p spiders-css-lsp --release --manifest-path "$ROOT_DIR/Cargo.toml"
mkdir -p "$BIN_DIR"
command cp -f "$ROOT_DIR/target/release/spiders-css-lsp" "$BIN_DIR/spiders-css-lsp"
chmod +x "$BIN_DIR/spiders-css-lsp"

pnpm --dir "$EXT_DIR" run sync:icon
