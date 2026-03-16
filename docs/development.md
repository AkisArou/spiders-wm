# Development And Debugging

## Common Commands

- `cargo check`
- `cargo test`
- `cargo run -p spiders-cli -- check-config`
- `cargo run -p spiders-cli -- build-config`
- `cargo run -p spiders-cli -- bootstrap-trace`
- `cargo run -p spiders-cli -- winit-run`

## Config Workflow

Use `check-config` to validate authored config and layout modules.

```bash
cargo run -p spiders-cli -- check-config
```

Use `build-config` to refresh prepared runtime output.

```bash
cargo run -p spiders-cli -- build-config
```

## Bootstrap Tracing

`bootstrap-trace` exercises controller startup without needing a full nested session.

```bash
cargo run -p spiders-cli -- bootstrap-trace
```

You can also feed a recorded script:

```bash
cargo run -p spiders-cli -- bootstrap-trace --transcript path/to/script.json
```

## Nested Winit Run

Start a nested compositor for local development:

```bash
cargo run -p spiders-cli -- winit-run
```

Optional flags:

- `--json`
- `--socket-name <name>`

Useful environment variables:

- `SPIDERS_WM_WINIT_DEBUG_SNAPSHOT_PATH`
- `SPIDERS_WM_WINIT_EXIT_AFTER_STARTUP`

If `SPIDERS_WM_WINIT_DEBUG_SNAPSHOT_PATH` is set, the CLI writes a debug snapshot
containing runtime state, controller state, current layout, window placements,
and titlebar planning.

## Discovery And Environment

Config discovery can be overridden with:

- `SPIDERS_WM_HOME`
- `SPIDERS_WM_CONFIG_DIR`
- `SPIDERS_WM_CACHE_DIR`
- `SPIDERS_WM_AUTHORED_CONFIG`

IPC clients can use:

- `SPIDERS_WM_IPC_SOCKET`

## Suggested Debug Loop

1. Run `cargo check` after Rust changes.
2. Run `cargo run -p spiders-cli -- check-config` after config or SDK changes.
3. Run `cargo run -p spiders-cli -- winit-run` for interactive compositor checks.
4. Capture a snapshot with `SPIDERS_WM_WINIT_DEBUG_SNAPSHOT_PATH` when layout or focus behavior looks wrong.

## Debugging Layouts

- validate authored config first
- keep layout modules small and deterministic
- use explicit `id` and `class` values so CSS and snapshots are easier to read
- prefer exact `match` clauses while debugging window assignment

## Debugging IPC

Useful commands:

- `cargo run -p spiders-cli -- ipc-smoke`
- `cargo run -p spiders-cli -- ipc-query --query state`
- `cargo run -p spiders-cli -- ipc-monitor --topic all`

See `docs/ipc.md` and `docs/cli.md` for the full command surface.
