default:
    @just --list

check-config:
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    cargo run -p spiders-cli -- check-config --json

test:
    cargo test

river-test:
    export WAYLAND_DEBUG=1 SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build"; \
    cargo build -p spiders-river && river -c '{{justfile_directory()}}/target/debug/spiders-river'