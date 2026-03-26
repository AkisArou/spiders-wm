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

if ! command -v wtype >/dev/null 2>&1; then
    echo "wm2 smoke harness requires 'wtype' in PATH" >&2
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

send_alt_shortcut() {
    local stderr_file
    stderr_file="$(mktemp)"
    if WAYLAND_DISPLAY="$SOCKET_NAME" wtype -M alt "$1" -m alt >/dev/null 2>"$stderr_file"; then
        rm -f "$stderr_file"
        return 0
    fi

    if grep -q 'Compositor does not support the virtual keyboard protocol' "$stderr_file"; then
        rm -f "$stderr_file"
        return 2
    fi

    cat "$stderr_file" >&2
    rm -f "$stderr_file"
    return 1
}

run_client
sleep 0.5
run_client
sleep 0.5
run_client
sleep 0.5

workspace_shortcut_smoke=0
if send_alt_shortcut 2; then
    workspace_shortcut_smoke=1
    sleep 0.5
    run_client
    sleep 0.5
    send_alt_shortcut w
    sleep 0.5
elif [[ $? -eq 2 ]]; then
    echo "wm2 workspace shortcut smoke skipped: nested compositor does not support virtual keyboard protocol" >&2
else
    echo "wm2 workspace shortcut smoke failed to inject Alt shortcut" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

if ! kill -0 "$WM2_PID" 2>/dev/null; then
    echo "spiders-wm2 exited during open-close-open-workspace smoke sequence" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

if [[ "$workspace_shortcut_smoke" -eq 1 ]]; then
    if ! grep -q 'selected workspace workspace=2' "$LOG_FILE"; then
        echo "wm2 smoke harness did not observe explicit workspace 2 selection" >&2
        cat "$LOG_FILE" >&2
        exit 1
    fi

    if ! grep -q 'selected workspace workspace=1' "$LOG_FILE"; then
        echo "wm2 smoke harness did not observe next-workspace wrap back to workspace 1" >&2
        cat "$LOG_FILE" >&2
        exit 1
    fi
fi

echo "wm2 smoke sequence passed on socket $SOCKET_NAME"