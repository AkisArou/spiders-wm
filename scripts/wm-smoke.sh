#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_FILE="${WM_SMOKE_LOG_FILE:-$(mktemp -t wm-smoke.XXXXXX.log)}"
SMOKE_ROOT="$(mktemp -d -t wm-smoke.XXXXXX)"
AUTHORED_CONFIG="${SPIDERS_WM_AUTHORED_CONFIG:-$ROOT_DIR/test_config/config.ts}"
CACHE_DIR="$SMOKE_ROOT/cache"
IPC_SOCKET_PATH="$SMOKE_ROOT/wm.sock"
CLIENT_LOG_DIR="$SMOKE_ROOT/clients"
SMOKE_MODE="${WM_SMOKE_MODE:-default}"

case "$SMOKE_MODE" in
    default)
        : "${WM_SMOKE_OPEN_COUNT:=4}"
        : "${WM_SMOKE_CLIENT_SETTLE_DELAY:=0.4}"
        : "${WM_SMOKE_OPEN_GAP:=0.8}"
        : "${WM_SMOKE_CLOSE_COUNT:=$WM_SMOKE_OPEN_COUNT}"
        : "${WM_SMOKE_CLOSE_SETTLE_DELAY:=0.7}"
        : "${WM_SMOKE_ENABLE_WORKSPACE_SHORTCUTS:=1}"
        : "${WM_SMOKE_ENABLE_SPAWN_SHORTCUT:=1}"
        ;;
    burst)
        : "${WM_SMOKE_OPEN_COUNT:=10}"
        : "${WM_SMOKE_CLIENT_SETTLE_DELAY:=0.03}"
        : "${WM_SMOKE_OPEN_GAP:=0.03}"
        : "${WM_SMOKE_CLOSE_COUNT:=$WM_SMOKE_OPEN_COUNT}"
        : "${WM_SMOKE_CLOSE_SETTLE_DELAY:=0.2}"
        : "${WM_SMOKE_ENABLE_WORKSPACE_SHORTCUTS:=0}"
        : "${WM_SMOKE_ENABLE_SPAWN_SHORTCUT:=0}"
        ;;
    *)
        echo "wm smoke harness does not recognize WM_SMOKE_MODE=$SMOKE_MODE" >&2
        exit 1
        ;;
esac

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

    if [[ "$status" -eq 0 ]]; then
        rm -rf "$SMOKE_ROOT"
    else
        echo "wm smoke artifacts preserved at $SMOKE_ROOT" >&2
    fi

    return "$status"
}
trap cleanup EXIT

mkdir -p "$CACHE_DIR"
mkdir -p "$CLIENT_LOG_DIR"

if [[ ! -f "$AUTHORED_CONFIG" ]]; then
    echo "wm smoke harness could not find authored config at $AUTHORED_CONFIG" >&2
    exit 1
fi

if ! command -v foot >/dev/null 2>&1; then
    echo "wm smoke harness requires 'foot' in PATH" >&2
    exit 1
fi

if ! command -v wtype >/dev/null 2>&1; then
    echo "wm smoke harness requires 'wtype' in PATH" >&2
    exit 1
fi

cargo build -p spiders-wm -p spiders-cli >/dev/null

WM_CMD=(target/debug/spiders-wm)
if command -v stdbuf >/dev/null 2>&1; then
    WM_CMD=(stdbuf -oL -eL "${WM_CMD[@]}")
fi

SPIDERS_WM_AUTHORED_CONFIG="$AUTHORED_CONFIG" \
SPIDERS_WM_CACHE_DIR="$CACHE_DIR" \
SPIDERS_WM_IPC_SOCKET="$IPC_SOCKET_PATH" \
SPIDERS_LOG="${SPIDERS_LOG:-spiders_wm=debug,spiders_cli=info}" \
"${WM_CMD[@]}" >"$LOG_FILE" 2>&1 &
WM_PID=$!

SOCKET_NAME=""
for _ in $(seq 1 100); do
    if ! kill -0 "$WM_PID" 2>/dev/null; then
        echo "spiders-wm exited during startup" >&2
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
    echo "failed to discover nested wm socket" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

for _ in $(seq 1 100); do
    if [[ -S "$IPC_SOCKET_PATH" ]]; then
        break
    fi
    if ! kill -0 "$WM_PID" 2>/dev/null; then
        echo "spiders-wm exited during waiting for IPC socket" >&2
        cat "$LOG_FILE" >&2
        exit 1
    fi
    sleep 0.1
done

if [[ ! -S "$IPC_SOCKET_PATH" ]]; then
    echo "failed to discover wm IPC socket at $IPC_SOCKET_PATH" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

run_client() {
    local client_index="${#CLIENT_PIDS[@]}"
    local client_log="$CLIENT_LOG_DIR/client-$client_index.log"

    WAYLAND_DISPLAY="$SOCKET_NAME" foot -e sh -lc 'trap : TERM INT; sleep 60' >"$client_log" 2>&1 &
    local client_pid=$!
    CLIENT_PIDS+=("$client_pid")
    sleep "$WM_SMOKE_CLIENT_SETTLE_DELAY"
    if ! kill -0 "$client_pid" 2>/dev/null; then
        echo "wm smoke harness client exited immediately (pid=$client_pid, log=$client_log)" >&2
        cat "$client_log" >&2 || true
        exit 1
    fi
    echo "$client_pid"
}

run_ipc_command() {
    target/debug/spiders-cli ipc-command --socket "$IPC_SOCKET_PATH" --command "$1" >/dev/null
}

assert_wm_alive() {
    local phase="$1"

    if ! kill -0 "$WM_PID" 2>/dev/null; then
        echo "spiders-wm exited during $phase" >&2
        tail -n 200 "$LOG_FILE" >&2 || true
        exit 1
    fi
}

close_one_window() {
    local step="$1"

    run_ipc_command close-focused-window
    sleep "$WM_SMOKE_CLOSE_SETTLE_DELAY"
    assert_wm_alive "close step $step"
}

log_count() {
    local pattern="$1"
    perl -pe 's/\e\[[0-9;]*m//g' "$LOG_FILE" | grep -c "$pattern" || true
}

wait_for_log_count() {
    local pattern="$1"
    local expected_count="$2"
    local timeout_seconds="$3"
    local description="$4"
    local waited=0

    while (( waited < timeout_seconds * 10 )); do
        local current_count
        current_count="$(log_count "$pattern")"
        if [[ "$current_count" -ge "$expected_count" ]]; then
            return 0
        fi

        assert_wm_alive "$description"
        sleep 0.1
        waited=$((waited + 1))
    done

    echo "wm smoke harness timed out waiting for $description: expected $expected_count matches for '$pattern'" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    return 1
}

max_metric_for_pattern() {
    local pattern="$1"
    local key="$2"

    local result
    result="$({
        perl -pe 's/\e\[[0-9;]*m//g' "$LOG_FILE" | grep "$pattern" || true
    } | {
        grep -o "${key}=[0-9.]*" || true
    } | cut -d= -f2 | sort -nr | head -n1)"

    printf '%s\n' "${result:-n/a}"
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

for _ in $(seq 1 "$WM_SMOKE_OPEN_COUNT"); do
    run_client >/dev/null
    sleep "$WM_SMOKE_OPEN_GAP"
    assert_wm_alive "opening clients"
done

wait_for_log_count 'wm added window' "$WM_SMOKE_OPEN_COUNT" 15 'initial window-add events'
wait_for_log_count 'wm first map commit' "$WM_SMOKE_OPEN_COUNT" 15 'initial first-map commits'

spawn_shortcut_smoke=0
if [[ "$WM_SMOKE_ENABLE_SPAWN_SHORTCUT" -eq 1 ]]; then
    shortcut_status=0
    if send_alt_shortcut Return; then
        spawn_shortcut_smoke=1
        sleep 0.8
        wait_for_log_count 'wm added window' "$((WM_SMOKE_OPEN_COUNT + 1))" 15 'spawn shortcut window-add event'
        wait_for_log_count 'wm first map commit' "$((WM_SMOKE_OPEN_COUNT + 1))" 15 'spawn shortcut first-map commit'
    else
        shortcut_status=$?
        if [[ "$shortcut_status" -eq 2 ]]; then
            echo "wm spawn shortcut smoke skipped: nested compositor does not support virtual keyboard protocol" >&2
        else
            echo "wm spawn shortcut smoke failed to inject Alt+Return" >&2
            cat "$LOG_FILE" >&2
            exit 1
        fi
    fi
fi

for step in $(seq 1 "$WM_SMOKE_CLOSE_COUNT"); do
    close_one_window "$step"
done

workspace_shortcut_smoke=0
if [[ "$WM_SMOKE_ENABLE_WORKSPACE_SHORTCUTS" -eq 1 ]]; then
    shortcut_status=0
    if send_alt_shortcut 2; then
        workspace_shortcut_smoke=1
        sleep 0.5
        run_client >/dev/null
        wait_for_log_count 'wm added window' "$((WM_SMOKE_OPEN_COUNT + 1))" 15 'workspace smoke window-add event'
        wait_for_log_count 'wm first map commit' "$((WM_SMOKE_OPEN_COUNT + 1))" 15 'workspace smoke first-map commit'
        sleep 0.5
        send_alt_shortcut 1
        sleep 0.5
    else
        shortcut_status=$?
        if [[ "$shortcut_status" -eq 2 ]]; then
            echo "wm workspace shortcut smoke skipped: nested compositor does not support virtual keyboard protocol" >&2
        else
            echo "wm workspace shortcut smoke failed to inject Alt shortcut" >&2
            cat "$LOG_FILE" >&2
            exit 1
        fi
    fi
fi

if ! kill -0 "$WM_PID" 2>/dev/null; then
    echo "spiders-wm exited during open-close-open-workspace smoke sequence" >&2
    cat "$LOG_FILE" >&2
    exit 1
fi

open_count="$(grep -c 'wm added window' "$LOG_FILE" || true)"
close_start_count="$(grep -c 'wm close start' "$LOG_FILE" || true)"
close_unmap_count="$(grep -c 'wm compositor observed root unmap commit' "$LOG_FILE" || true)"
relayout_count="$(grep -c 'wm relayout start' "$LOG_FILE" || true)"
first_map_count="$(grep -c 'wm first map commit' "$LOG_FILE" || true)"
deferred_first_map_count="$(grep -c 'wm deferred first-map relayout while additional windows are still pending map' "$LOG_FILE" || true)"
prepared_snapshot_count="$(grep -c 'wm prepared titlebar snapshot for window' "$LOG_FILE" || true)"
reused_snapshot_count="$(grep -c 'wm reused prepared titlebar snapshot for window' "$LOG_FILE" || true)"
max_relayout_elapsed_ms="$(max_metric_for_pattern 'wm relayout finished' 'elapsed_ms')"
max_overlay_elapsed_ms="$(max_metric_for_pattern 'wm relayout finished' 'overlay_elapsed_ms')"
max_prepare_elapsed_ms="$(max_metric_for_pattern 'wm prepared titlebar snapshot for window' 'elapsed_ms')"

if [[ "$open_count" -lt "$WM_SMOKE_OPEN_COUNT" ]]; then
    echo "wm smoke harness expected at least $WM_SMOKE_OPEN_COUNT window-add events, saw $open_count" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    exit 1
fi

if [[ "$close_start_count" -lt "$WM_SMOKE_CLOSE_COUNT" ]]; then
    echo "wm smoke harness expected at least $WM_SMOKE_CLOSE_COUNT close-start events, saw $close_start_count" >&2
    tail -n 200 "$LOG_FILE" >&2 || true
    exit 1
fi

if [[ "$workspace_shortcut_smoke" -eq 1 ]]; then
    if ! grep -Eq 'selected workspace.*workspace.*2' "$LOG_FILE"; then
        echo "wm smoke harness did not observe explicit workspace 2 selection" >&2
        cat "$LOG_FILE" >&2
        exit 1
    fi

    if ! grep -Eq 'selected workspace.*workspace.*1' "$LOG_FILE"; then
        echo "wm smoke harness did not observe next-workspace wrap back to workspace 1" >&2
        cat "$LOG_FILE" >&2
        exit 1
    fi
fi

echo "wm smoke sequence passed on socket $SOCKET_NAME"
echo "wm smoke log: $LOG_FILE"
echo "wm smoke summary: mode=$SMOKE_MODE spawn_shortcut=$spawn_shortcut_smoke workspace_shortcut=$workspace_shortcut_smoke opens=$open_count first_maps=$first_map_count deferred_first_maps=$deferred_first_map_count close_starts=$close_start_count close_unmaps=$close_unmap_count relayouts=$relayout_count prepared_snapshots=$prepared_snapshot_count reused_snapshots=$reused_snapshot_count max_prepare_ms=${max_prepare_elapsed_ms:-n/a} max_overlay_ms=${max_overlay_elapsed_ms:-n/a} max_relayout_ms=${max_relayout_elapsed_ms:-n/a}"
