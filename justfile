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
    cargo run -p spiders-wm2

wm2-smoke:
    ./scripts/wm2-smoke.sh