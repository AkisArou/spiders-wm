# Spiders River Plan

## Goal

Build `spiders-river` as a river-backed window manager client that reuses the existing spiders config, JavaScript layout runtime, shared state model, and IPC protocol while dropping compositor ownership from `spiders-wm2`.

## Why This Pivot

- river already provides the compositor, rendering, and protocol support that `spiders-wm2` struggled to stabilize.
- spiders can focus on policy: layouts, bindings, workspace/tag behavior, config reload, and IPC.
- this matches the repository's JS-driven layout/runtime strengths better than continuing a full compositor rewrite.

## Reference

- Primary river implementation reference: `/home/akisarou/projects/orilla`
- Core protocol/event-loop shape to mirror:
  - `/home/akisarou/projects/orilla/crates/orilla/src/lib.rs`
  - `/home/akisarou/projects/orilla/crates/orilla/src/protocol.rs`

## Reuse Targets

- `crates/spiders-config`
- `crates/runtimes/js`
- `crates/spiders-scene`
- `crates/spiders-shared`
- `crates/spiders-ipc`

## Avoid Reusing

- `crates/spiders-wm2` smithay runtime, handlers, render loop, and transaction/presentation code
- most compositor-topology abstractions from `crates/spiders-wm`

## Milestone 1: Bootstrap

- Create the `spiders-river` crate
- Load authored/prepared config through the existing JS runtime
- Define river-facing runtime state for outputs, seats, windows, and workspaces/tags
- Introduce a protocol module that centralizes required river globals
- Introduce an action bridge from `spiders_shared::api::WmAction` to river-specific commands

Exit criteria:

- crate builds and tests
- config paths and config loading work
- runtime state can be initialized from config workspace definitions

## Milestone 2: River Connection

- Connect to the compositor through `wayland-client`
- Bind `river_window_manager_v1`
- Track registry discovery and protocol availability
- Create a real event loop shell similar to orilla's `blocking_dispatch()` model

Exit criteria:

- detect whether required river globals are available
- start a live session and stay connected to river without crashing

## Milestone 3: Window/Output/Seat State

- mirror river outputs into `OutputSnapshot`-compatible state
- mirror river windows into `WindowSnapshot`-compatible state
- track focused window, current output, and current workspace/tag mapping
- translate river lifecycle events into `CompositorEvent`s for IPC subscribers

Exit criteria:

- state snapshots update live when windows open/close/focus
- IPC queries can return current state without smithay

## Milestone 4: Actions and Bindings

- implement a first supported subset:
  - spawn
  - close focused window
  - activate workspace/tag
  - focus window
  - toggle floating/fullscreen when river supports it cleanly
- map config bindings onto river seat/input protocol support

Exit criteria:

- configured bindings trigger real river actions
- focused window/workspace commands work live

## Milestone 5: Layout Evaluation

- build `StateSnapshot` and `LayoutEvaluationContext` from river state
- evaluate selected layouts with the existing JS runtime
- apply resolved layout rectangles through river's WM protocol
- support single-output first, then extend to multi-output

Exit criteria:

- windows are arranged by spiders layout evaluation under river
- layout switching works without restarting clients

## Milestone 6: IPC and Reload

- reuse `spiders-ipc` protocol definitions
- publish live state/events from the river runtime
- support config reload without restarting the compositor session

Exit criteria:

- CLI clients can query state and send actions
- config reload updates behavior live

## Open Design Questions

- How should spiders workspaces map onto river tags when multiple tags are visible?
- Which current `WmAction` variants should be preserved exactly vs adapted for river semantics?
- What parts of effects/titlebar styling remain meaningful in a river-client architecture?

## Immediate Next Step

Implement Milestone 2 by replacing the placeholder backend/protocol scaffolding with a real registry-binding event loop for `river_window_manager_v1`.
