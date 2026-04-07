# CLI

The main entry point is `spiders-cli`. It provides CLI tooling for config validation, building, IPC queries, and compositor debug dumps.

Run commands with:

```bash
cargo run -p spiders-cli -- <command>
```

The compositor runtime is in `crates/spiders-wm` and is launched via:

```bash
just dev
```

or directly:

```bash
cargo build -p spiders-wm && river -c ./target/debug/spiders-wm
```

## General

Without a subcommand, the CLI prints config discovery information.

Global options:

- `--json`

## Config Commands

### `check-config`

Validates config loading and layout modules.

```bash
cargo run -p spiders-cli -- check-config
```

### `build-config`

Builds or refreshes the prepared config cache.

```bash
cargo run -p spiders-cli -- build-config
```

### `bootstrap-trace`

Runs startup/controller tracing.

Options:

- `--events <path>`
- `--transcript <path>`

```bash
cargo run -p spiders-cli -- bootstrap-trace --transcript trace.json
```

### `winit-run`

Starts a nested development compositor.

Options:

- `--socket-name <name>`

```bash
cargo run -p spiders-cli -- winit-run --socket-name spiders-dev
```

## IPC Commands

### `ipc-smoke`

Exercises IPC framing and server handling without a live compositor.

```bash
cargo run -p spiders-cli -- ipc-smoke
```

### `ipc-query`

Options:

- `--socket <path>`
- `--query <name>`

Examples:

```bash
cargo run -p spiders-cli -- ipc-query --query state
cargo run -p spiders-cli -- ipc-query --query workspace-names
```

### `ipc-action`

Options:

- `--socket <path>`
- `--action <name>`

Examples:

```bash
cargo run -p spiders-cli -- ipc-action --action reload-config
cargo run -p spiders-cli -- ipc-action --action view-workspace:3
cargo run -p spiders-cli -- ipc-action --action set-layout:columns
```

`view-workspace:<n>` and `toggle-view-workspace:<n>` accept `1` through `9`.

### `ipc-monitor`

Options:

- `--socket <path>`
- `--topic <name>` repeatable

Examples:

```bash
cargo run -p spiders-cli -- ipc-monitor --topic all
cargo run -p spiders-cli -- ipc-monitor --topic workspaces --topic layout
```

### `ipc-debug`

Requests an on-demand debug dump from a running `spiders-wm` instance.

Options:

- `--socket <path>`
- `--dump <name>`

Supported dump names:

- `wm-state`
- `debug-profile`
- `scene-snapshot`
- `frame-sync`
- `titlebar-overlays`
- `seats`

Examples:

```bash
cargo run -p spiders-cli -- ipc-debug --dump wm-state
cargo run -p spiders-cli -- ipc-debug --dump scene-snapshot --json
cargo run -p spiders-cli -- ipc-debug --socket "$SPIDERS_WM_IPC_SOCKET" --dump debug-profile
```

When `SPIDERS_WM_DEBUG_OUTPUT_DIR` is configured and debug output is enabled in the compositor, the response includes the written file path.

## Environment

Useful environment variables:

**Config Discovery:**
- `SPIDERS_WM_HOME` - home directory base for config/cache
- `SPIDERS_WM_CONFIG_DIR` - config search directory (default: `~/.config/spiders-wm`)
- `SPIDERS_WM_CACHE_DIR` - runtime output cache directory
- `SPIDERS_WM_AUTHORED_CONFIG` - direct path to config.ts/config.js file

**Runtime:**
- `SPIDERS_WM_IPC_SOCKET` - socket path for IPC communication
- `SPIDERS_WM_DEBUG_PROFILE` - compositor debug profile: `minimal`, `protocol`, `render`, or `full`
- `SPIDERS_WM_DEBUG_OUTPUT_DIR` - directory for compositor debug dumps

**Logging:**
- `SPIDERS_LOG` - tracing filter (e.g., `warn,spiders_wm=debug`)
- `RUST_LOG` - fallback tracing filter

**Development:**
- `SPIDERS_WM_WINIT_DEBUG_SNAPSHOT_PATH` - save window tree snapshots
- `SPIDERS_WM_WINIT_EXIT_AFTER_STARTUP` - exit compositor after init

## Nested Wayland Debugging

For protocol and lifecycle debugging, run `spiders-wm` nested with a debug profile and then launch clients against that nested compositor.

Start the compositor:

```bash
SPIDERS_WM_DEBUG_PROFILE=protocol \
SPIDERS_WM_DEBUG_OUTPUT_DIR="$PWD/.spiders-wm-debug" \
SPIDERS_LOG=debug \
just dev
```

For the fully-instrumented setup, use:

```bash
just dev-debug
```

For the existing open/close smoke sequence with full debug artifacts enabled, use:

```bash
just wm-debug-smoke
```

`apps/spiders-wm` logs the nested `WAYLAND_DISPLAY` and `SPIDERS_WM_IPC_SOCKET` values during startup. Use those for clients and IPC tools.

Run a client with Wayland protocol tracing enabled:

```bash
WAYLAND_DISPLAY=<nested-display> \
WAYLAND_DEBUG=1 \
foot
```

Capture compositor-side dumps while reproducing the issue:

```bash
SPIDERS_WM_IPC_SOCKET=<nested-ipc-socket> \
cargo run -p spiders-cli -- ipc-debug --dump wm-state --json

SPIDERS_WM_IPC_SOCKET=<nested-ipc-socket> \
cargo run -p spiders-cli -- ipc-debug --dump scene-snapshot --json

SPIDERS_WM_IPC_SOCKET=<nested-ipc-socket> \
cargo run -p spiders-cli -- ipc-debug --dump frame-sync --json
```

Recommended repro loop:

1. Start nested `spiders-wm` with `SPIDERS_WM_DEBUG_PROFILE=protocol` or `full`.
2. Launch the target client with `WAYLAND_DISPLAY=<nested-display>` and `WAYLAND_DEBUG=1`.
3. Reproduce the bug.
4. Capture `wm-state`, `scene-snapshot`, `frame-sync`, or `titlebar-overlays` dumps over IPC.
5. Correlate the client-side `WAYLAND_DEBUG` log with compositor logs and dump timestamps.

With `SPIDERS_WM_DEBUG_PROFILE=protocol`, the compositor now emits structured lifecycle logs around focus requests, backend focus application, initial configure sends, popup configure sends, and root commits. With `render` or `full`, it also emits map/unmap/commit render lifecycle logs.
