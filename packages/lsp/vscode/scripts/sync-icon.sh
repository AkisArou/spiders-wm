#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(dirname "$0")/../../../.."
SRC="$ROOT_DIR/assets/spiders-wm-mark.svg"
DST="$ROOT_DIR/packages/lsp/vscode/media/icon.png"

rsvg-convert -w 256 -h 256 "$SRC" -o "$DST"
