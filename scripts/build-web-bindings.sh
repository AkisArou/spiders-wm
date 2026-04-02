#!/usr/bin/env bash

set -euo pipefail

profile="${1:-dev}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
crate_dir="${repo_root}/crates/spiders-web-bindings"
out_dir="${repo_root}/apps/spiders-wm-playground/src/generated/spiders-web-bindings"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "error: wasm-pack is required to build spiders-web-bindings" >&2
  echo "install it with: cargo install wasm-pack" >&2
  exit 1
fi

if ! rustup target list --installed | grep -q '^wasm32-unknown-unknown$'; then
  echo "error: Rust target wasm32-unknown-unknown is required" >&2
  echo "install it with: rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

args=(build "${crate_dir}" --target web --out-dir "${out_dir}")

case "${profile}" in
  dev)
    args+=(--dev)
    ;;
  release)
    ;;
  *)
    echo "error: expected profile 'dev' or 'release', got '${profile}'" >&2
    exit 1
    ;;
esac

wasm-pack "${args[@]}"