default:
    @just --list

check-config:
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    cargo run -p spiders-cli -- check-config --json

test:
    cargo test

dev:
    export SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" SPIDERS_LOG="${SPIDERS_LOG:-warn,spiders_=debug}"; \
    cargo build -p spiders-wm && river -c '{{justfile_directory()}}/target/debug/spiders-wm'