default:
    @just --list

check-config:
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    cargo run -p spiders-cli -- check-config --json

test:
    cargo test

dev:
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    RUST_LOG=debug \
    cargo run -p spiders-wm

x-dev:
    Xephyr :1 -screen 1440x900 -resizeable -name spiders-wm-x -title "spiders-wm-x test display" -ac -br -reset

x-run:
    DISPLAY=:1 \
    WAYLAND_DISPLAY= \
    SWAYSOCK= \
    XDG_SESSION_TYPE=x11 \
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    RUST_LOG=debug \
    cargo run -p spiders-wm-x -- --manage

x-session:
    sh -c 'Xephyr :1 -screen 1440x900 -resizeable -name spiders-wm-x -title "spiders-wm-x test display" -ac -br -reset & sleep 1 && DISPLAY=:1 WAYLAND_DISPLAY= SWAYSOCK= XDG_SESSION_TYPE=x11 SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" RUST_LOG=debug cargo run -p spiders-wm-x -- --manage'

x-clients:
    DISPLAY=:1 xterm & \
    DISPLAY=:1 xclock & \
    DISPLAY=:1 xeyes & \
    wait

x-dump-state:
    DISPLAY=:1 \
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    RUST_LOG=debug \
    cargo run -p spiders-wm-x -- --dump-state

dev-debug:
    mkdir -p "$PWD/.spiders-wm-debug"
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    SPIDERS_WM_DEBUG_PROFILE=full \
    SPIDERS_WM_DEBUG_OUTPUT_DIR="$PWD/.spiders-wm-debug" \
    SPIDERS_LOG=debug \
    cargo run -p spiders-wm

wm-smoke:
    ./scripts/wm-smoke.sh

wm-x-smoke:
    ./scripts/wm-x-smoke.sh

wm-debug-smoke:
    mkdir -p "$PWD/.spiders-wm-debug"
    SPIDERS_WM_DEBUG_PROFILE=full \
    SPIDERS_WM_DEBUG_OUTPUT_DIR="$PWD/.spiders-wm-debug" \
    SPIDERS_LOG=debug \
    ./scripts/wm-smoke.sh

wm-live-smoke:
    SPIDERS_WM_RUN_LIVE_SMOKE=1 cargo test -p spiders-wm --test live_ipc_smoke -- --ignored --nocapture

www-dev:
    cd apps/spiders-wm-www && trunk serve --open
