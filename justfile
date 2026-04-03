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

wm-smoke:
    ./scripts/wm-smoke.sh

wm-live-smoke:
    SPIDERS_WM_RUN_LIVE_SMOKE=1 cargo test -p spiders-wm --test live_ipc_smoke -- --ignored --nocapture

www-dev:
    cd apps/spiders-wm-www && trunk serve --open