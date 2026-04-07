# IPC

`spiders-wm` exposes a local IPC surface for queries, actions, debug requests, and event subscriptions.

## Transport

The current protocol is newline-delimited JSON over a Unix domain socket.

- one JSON message per line
- request and response envelopes can carry `request_id`
- clients send queries, actions, debug requests, subscribe, and unsubscribe messages
- servers reply with query results, debug responses, action acknowledgements, events, and errors

The default socket path can be supplied through `SPIDERS_WM_IPC_SOCKET`.

## Queries

Supported query names:

- `state`
- `focused-window`
- `current-output`
- `current-workspace`
- `monitor-list`
- `workspace-names`

## Actions

Supported action names:

- `reload-config`
- `set-layout:<name>`
- `cycle-layout-next`
- `cycle-layout-previous`
- `view-workspace:<1-9>`
- `toggle-view-workspace:<1-9>`
- `activate-workspace:<workspace-id>`
- `assign-workspace:<workspace-id>@<output-id>`
- `spawn:<command>`
- `toggle-floating`
- `toggle-fullscreen`
- `focus-left`
- `focus-right`
- `focus-up`
- `focus-down`
- `close-focused-window`

Workspace action arguments are workspace names.
Shortcut workspace actions use numeric indices `1` through `9`.

## Debug Requests

Supported debug dump kinds:

- `wm-state`
- `debug-profile`
- `scene-snapshot`
- `frame-sync`
- `titlebar-overlays`
- `seats`

The recommended client surface is:

```bash
cargo run -p spiders-cli -- ipc-debug --dump wm-state
```

If the compositor is running with `SPIDERS_WM_DEBUG_PROFILE` enabled, it can also persist these dumps to `SPIDERS_WM_DEBUG_OUTPUT_DIR`.

## Subscription Topics

Supported coarse topics:

- `all`
- `focus`
- `windows`
- `workspaces`
- `layout`
- `config`

`all` dominates more specific topics.

## Events

Current event names:

- `focus-change`
- `window-created`
- `window-destroyed`
- `window-workspace-change`
- `window-floating-change`
- `window-fullscreen-change`
- `workspace-change`
- `layout-change`
- `config-reloaded`

## Envelope Shape

Typical client envelope:

```json
{
  "request_id": "req-1",
  "message": {
    "type": "query",
    "payload": "state"
  }
}
```

Typical subscribe envelope:

```json
{
  "request_id": "req-2",
  "message": {
    "type": "subscribe",
    "payload": {
      "topics": ["workspaces", "layout"]
    }
  }
}
```

Typical debug dump envelope:

```json
{
  "request_id": "req-3",
  "message": {
    "type": "debug",
    "payload": {
      "type": "dump",
      "payload": {
        "kind": "scene-snapshot"
      }
    }
  }
}
```

## Notes

- IPC payloads use serializable snapshots and typed actions
- debug dump responses return typed dump metadata, not raw compositor handles
- raw compositor or Wayland handles are not exposed
- workspace subscriptions track workspace-change events, not low-level backend events
- Focus actions (`focus-left`, `focus-right`, `focus-up`, `focus-down`) navigate between windows without reordering; they are distinct from swap/move actions
- Keybindings are deduplicated semantically (same modifiers + keysym = single binding, even if trigger text differs like `alt+Return` vs `Alt+Enter`)

## Nested Protocol Debugging

The intended workflow for Smithay/Wayland debugging is:

1. Start nested `spiders-wm` with `SPIDERS_WM_DEBUG_PROFILE=protocol` or `full`.
2. Read the logged `WAYLAND_DISPLAY` and `SPIDERS_WM_IPC_SOCKET` values.
3. Launch one or more clients against that nested display with `WAYLAND_DEBUG=1`.
4. Capture compositor dumps through `spiders-cli ipc-debug` while reproducing the issue.

Example:

```bash
WAYLAND_DISPLAY=<nested-display> WAYLAND_DEBUG=1 foot
SPIDERS_WM_IPC_SOCKET=<nested-ipc-socket> cargo run -p spiders-cli -- ipc-debug --dump seats --json
```
