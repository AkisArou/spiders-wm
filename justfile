default:
    @just --list

winit-run:
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    SPIDERS_WM_WINIT_DEBUG_SNAPSHOT_PATH=/tmp/spiders-debug-snapshot.txt \
    cargo run -p spiders-cli -- winit-run --socket-name spiders-test

winit-run-wm2:
    SPIDERS_WM2_CONFIG_PATH="$PWD/test_config/config.ts" \
    cargo run -p spiders-wm2

foot:
    WAYLAND_DISPLAY=spiders-test foot

check-config:
    SPIDERS_WM_AUTHORED_CONFIG="$PWD/test_config/config.ts" \
    SPIDERS_WM_CACHE_DIR="$PWD/test_config/.spiders-wm-build" \
    cargo run -p spiders-cli -- check-config --json

test:
    cargo test

river-test:
    cargo build -p spiders-river && river -c '{{justfile_directory()}}/target/debug/spiders-river'

river-test-run:
    river -c 'cd {{justfile_directory()}} && cargo run -p spiders-river'
