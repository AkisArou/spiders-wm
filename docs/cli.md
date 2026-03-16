# CLI

The main entry point is `spiders-cli`.

Run commands with:

```bash
cargo run -p spiders-cli -- <command>
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

## Environment

Useful environment variables:

- `SPIDERS_WM_HOME`
- `SPIDERS_WM_CONFIG_DIR`
- `SPIDERS_WM_CACHE_DIR`
- `SPIDERS_WM_AUTHORED_CONFIG`
- `SPIDERS_WM_IPC_SOCKET`
- `SPIDERS_WM_WINIT_DEBUG_SNAPSHOT_PATH`
- `SPIDERS_WM_WINIT_EXIT_AFTER_STARTUP`
