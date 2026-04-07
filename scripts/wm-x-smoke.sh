#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_FILE="${WM_X_SMOKE_LOG_FILE:-$(mktemp -t wm-x-smoke.XXXXXX.log)}"
SMOKE_ROOT="$(mktemp -d -t wm-x-smoke.XXXXXX)"
AUTHORED_CONFIG="${SPIDERS_WM_AUTHORED_CONFIG:-$ROOT_DIR/test_config/config.ts}"
CACHE_DIR="$SMOKE_ROOT/cache"
DISPLAY_NUM="${WM_X_SMOKE_DISPLAY_NUM:-1}"
DISPLAY_NAME=":${DISPLAY_NUM}"
SCREEN_SIZE="${WM_X_SMOKE_SCREEN:-1440x900}"
CLIENT_LOG_DIR="$SMOKE_ROOT/clients"
: "${WM_X_SMOKE_OPEN_COUNT:=3}"
: "${WM_X_SMOKE_CLIENT_SETTLE_DELAY:=0.5}"
: "${WM_X_SMOKE_OPEN_GAP:=0.5}"

declare -a CLIENT_PIDS=()
cleanup() {
    local status=$?

    for client_pid in "${CLIENT_PIDS[@]:-}"; do
        if kill -0 "$client_pid" 2>/dev/null; then
            kill "$client_pid" 2>/dev/null || true
            wait "$client_pid" 2>/dev/null || true
        fi
    done
    if [[ -n "${WM_PID:-}" ]] && kill -0 "$WM_PID" 2>/dev/null; then
        kill "$WM_PID" 2>/dev/null || true
        wait "$WM_PID" 2>/dev/null || true
    fi
    if [[ -n "${XEPHYR_PID:-}" ]] && kill -0 "$XEPHYR_PID" 2>/dev/null; then
        kill "$XEPHYR_PID" 2>/dev/null || true
        wait "$XEPHYR_PID" 2>/dev/null || true
    fi

    if [[ "$status" -eq 0 ]]; then
        rm -rf "$SMOKE_ROOT"
    else
        echo "wm-x-smoke artifacts preserved at $SMOKE_ROOT" >&2
    fi

    return "$status"
}
trap cleanup EXIT

mkdir -p "$CACHE_DIR" "$CLIENT_LOG_DIR"

if [[ ! -f "$AUTHORED_CONFIG" ]]; then
    echo "wm-x-smoke could not find authored config at $AUTHORED_CONFIG" >&2
    exit 1
fi

for required in Xephyr alacritty xprop xdotool; do
    if ! command -v "$required" >/dev/null 2>&1; then
        echo "wm-x-smoke requires '$required' in PATH" >&2
        exit 1
    fi
done

cargo build -p spiders-wm-x >/dev/null

XEPHYR_CMD=(Xephyr "$DISPLAY_NAME" -screen "$SCREEN_SIZE" -resizeable -name spiders-wm-x -title "spiders-wm-x test display" -ac -br -reset)
if command -v stdbuf >/dev/null 2>&1; then
    XEPHYR_CMD=(stdbuf -oL -eL "${XEPHYR_CMD[@]}")
fi
"${XEPHYR_CMD[@]}" >"$SMOKE_ROOT/xephyr.log" 2>&1 &
XEPHYR_PID=$!

for _ in $(seq 1 100); do
    if [[ -e "/tmp/.X11-unix/X${DISPLAY_NUM}" ]]; then
        break
    fi
    if ! kill -0 "$XEPHYR_PID" 2>/dev/null; then
        echo "Xephyr exited during startup" >&2
        cat "$SMOKE_ROOT/xephyr.log" >&2 || true
        exit 1
    fi
    sleep 0.1
done

WM_CMD=(target/debug/spiders-wm-x --manage)
if command -v stdbuf >/dev/null 2>&1; then
    WM_CMD=(stdbuf -oL -eL "${WM_CMD[@]}")
fi

DISPLAY="$DISPLAY_NAME" \
WAYLAND_DISPLAY= \
SWAYSOCK= \
XDG_SESSION_TYPE=x11 \
SPIDERS_WM_AUTHORED_CONFIG="$AUTHORED_CONFIG" \
SPIDERS_WM_CACHE_DIR="$CACHE_DIR" \
RUST_LOG="${RUST_LOG:-debug}" \
"${WM_CMD[@]}" >"$LOG_FILE" 2>&1 &
WM_PID=$!

for _ in $(seq 1 100); do
    if ! kill -0 "$WM_PID" 2>/dev/null; then
        echo "spiders-wm-x exited during startup" >&2
        cat "$LOG_FILE" >&2 || true
        exit 1
    fi
    if grep -q 'entered X11 manage event loop' "$LOG_FILE"; then
        break
    fi
    sleep 0.1
done

assert_wm_alive() {
    local phase="$1"
    if ! kill -0 "$WM_PID" 2>/dev/null; then
        echo "spiders-wm-x exited during $phase" >&2
        tail -n 200 "$LOG_FILE" >&2 || true
        exit 1
    fi
}

wait_for_client_windows() {
    local expected="$1"
    for _ in $(seq 1 100); do
        local current
        current="$(DISPLAY="$DISPLAY_NAME" xprop -root _NET_CLIENT_LIST 2>/dev/null | grep -o '0x[0-9a-fA-F]\+' | wc -l || true)"
        if [[ "$current" -ge "$expected" ]]; then
            return 0
        fi
        assert_wm_alive "waiting for client windows"
        sleep 0.1
    done

    echo "wm-x-smoke timed out waiting for $expected X11 client windows" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    exit 1
}

active_window() {
    DISPLAY="$DISPLAY_NAME" xdotool getactivewindow 2>/dev/null || true
}

send_alt_shortcut() {
    local key="$1"
    DISPLAY="$DISPLAY_NAME" xdotool key --clearmodifiers Alt+"$key"
}

run_client() {
    local client_index="${#CLIENT_PIDS[@]}"
    local client_log="$CLIENT_LOG_DIR/client-$client_index.log"

    DISPLAY="$DISPLAY_NAME" \
    WAYLAND_DISPLAY= \
    SWAYSOCK= \
    XDG_SESSION_TYPE=x11 \
    alacritty -e sh -lc 'trap : TERM INT; sleep 60' >"$client_log" 2>&1 &
    local client_pid=$!
    CLIENT_PIDS+=("$client_pid")
    sleep "$WM_X_SMOKE_CLIENT_SETTLE_DELAY"
    if ! kill -0 "$client_pid" 2>/dev/null; then
        echo "wm-x-smoke client exited immediately (pid=$client_pid, log=$client_log)" >&2
        cat "$client_log" >&2 || true
        exit 1
    fi
}

for _ in $(seq 1 "$WM_X_SMOKE_OPEN_COUNT"); do
    run_client
    sleep "$WM_X_SMOKE_OPEN_GAP"
    assert_wm_alive "opening clients"
done

wait_for_client_windows "$WM_X_SMOKE_OPEN_COUNT"

initial_active_window="$(active_window)"
if [[ -z "$initial_active_window" ]]; then
    echo "wm-x-smoke could not determine initial active window" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    exit 1
fi

send_alt_shortcut h
sleep 0.4
assert_wm_alive "directional focus shortcut"
focused_after_h="$(active_window)"
if [[ -z "$focused_after_h" || "$focused_after_h" == "$initial_active_window" ]]; then
    echo "wm-x-smoke focus shortcut Alt+h did not change the active window" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    exit 1
fi

before_close_count="$(DISPLAY="$DISPLAY_NAME" xprop -root _NET_CLIENT_LIST 2>/dev/null | grep -o '0x[0-9a-fA-F]\+' | wc -l || true)"
send_alt_shortcut q
sleep 0.8
assert_wm_alive "close shortcut"
after_close_count="$(DISPLAY="$DISPLAY_NAME" xprop -root _NET_CLIENT_LIST 2>/dev/null | grep -o '0x[0-9a-fA-F]\+' | wc -l || true)"
if [[ "$after_close_count" -ge "$before_close_count" ]]; then
    echo "wm-x-smoke close shortcut Alt+q did not reduce the client window count" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    exit 1
fi

window_count="$(DISPLAY="$DISPLAY_NAME" xprop -root _NET_CLIENT_LIST 2>/dev/null | tr -cd ',' | wc -c)"
window_count=$((window_count + 1))

echo "wm-x-smoke passed on $DISPLAY_NAME"
echo "wm-x-smoke log: $LOG_FILE"
echo "wm-x-smoke xephyr log: $SMOKE_ROOT/xephyr.log"
echo "wm-x-smoke client count: $window_count"
echo "wm-x-smoke artifacts: $SMOKE_ROOT"
