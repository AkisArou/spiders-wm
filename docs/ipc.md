# IPC

`spiders-wm` exposes a local IPC surface for queries, actions, and event
subscriptions.

## Transport

The current protocol is newline-delimited JSON over a Unix domain socket.

- one JSON message per line
- request and response envelopes can carry `request_id`
- clients send queries, actions, subscribe, and unsubscribe messages
- servers reply with query results, action acknowledgements, events, and errors

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

## Notes

- IPC payloads use serializable snapshots and typed actions
- raw compositor or Wayland handles are not exposed
- workspace subscriptions track workspace-change events, not low-level backend events
