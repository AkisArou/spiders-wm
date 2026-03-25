#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_FILE="$(mktemp)"
cleanup() {
    if [[ -n "${WM2_PID:-}" ]] && kill -0 "$WM2_PID" 2>/dev/null; then
        kill "$WM2_PID" 2>/dev/null || true
        wait "$WM2_PID" 2>/dev/null || true
    fi
    rm -f "$LOG_FILE"
}
trap cleanup EXIT

if ! command -v foot >/dev/null 2>&1; then
    echo "wm2 smoke harness requires 'foot' in PATH" >&2
    exit 1
fi

cargo build -p spiders-wm2 >/dev/null

WM2_CMD=(target/debug/spiders-wm2)
if command -v stdbuf >/dev/null 2>&1; then
    WM2_CMD=(stdbuf -oL -eL "${WM2_CMD[@]}")
fi

SPIDERS_LOG="${SPIDERS_LOG:-info}" "${WM2_CMD[@]}" >"$LOG_FILE" 2>&1 &
WM2_PID=$!

SOCKET_NAME=""
for _ in $(seq 1 100); do
    if ! kill -0 "$WM2_PID" 2>/dev/null; then
        echo "spiders-wm2 exited during startup" >&2
        cat "$LOG_FILE" >&2
        exit 1
    fi

    SOCKET_NAME="$(grep -o 'wayland-[^"[:space:]]*' "$LOG_FILE" | tail -n1 || true)"
    if [[ -n "$SOCKET_NAME" ]]; then
        break
    fi
    sleep 0.1
done

if [[ -z "$SOCKET_NAME" ]]; then
    echo "failed to discover nested wm2 socket" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

run_client() {
    WAYLAND_DISPLAY="$SOCKET_NAME" foot -e sh -lc 'sleep 0.8' >/dev/null 2>&1 &
    local client_pid=$!
    wait "$client_pid"
}

run_client
sleep 0.5
run_client
sleep 0.5
run_client
sleep 0.5

if ! kill -0 "$WM2_PID" 2>/dev/null; then
    echo "spiders-wm2 exited during open-close-open smoke sequence" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

echo "wm2 smoke sequence passed on socket $SOCKET_NAME"