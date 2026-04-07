# Smithay Architecture

This document explains how `apps/spiders-wm` uses Smithay in this repository.

It is not a generic Smithay tutorial.

It is a codebase-specific architecture reference intended to make compositor-side changes safer.

## Purpose

`spiders-wm` is the real compositor host.

That means it is responsible for:

- owning Smithay state objects
- handling Wayland protocol lifecycle
- processing input
- managing mapped windows and workspaces
- translating runtime/model changes into compositor behavior
- scheduling relayout and rendering

The important architectural boundary is:

- `spiders-core` and `spiders-wm-runtime` contain WM logic and command/effect vocabulary
- `apps/spiders-wm` is the real Smithay host that performs compositor-side effects

## Entry Point

Main entry:

- `apps/spiders-wm/src/main.rs`

High-level startup flow:

1. initialize tracing/logging
2. create a `calloop::EventLoop<SpidersWm>`
3. create a Smithay `Display<SpidersWm>`
4. build `SpidersWm` state in `app::bootstrap::build_state(...)`
5. initialize the nested winit backend in `app::bootstrap::init_winit(...)`
6. publish nested sockets:
   - `WAYLAND_DISPLAY`
   - `SPIDERS_WM_IPC_SOCKET`
7. run the event loop

This app is currently a nested compositor using Smithay + winit.

## Central State

Main state type:

- `apps/spiders-wm/src/state.rs`
- `SpidersWm`

Important groups inside `SpidersWm`:

### Smithay / Wayland State

- `display_handle`
- `compositor_state`
- `xdg_shell_state`
- `shm_state`
- `dmabuf_state`
- `dmabuf_global`
- `data_device_state`
- `seat_state`
- `seat`
- `popups`

These are the protocol/server-side objects Smithay needs to run a compositor.

### Backend / Render State

- `backend: Option<WinitGraphicsBackend<GlesRenderer>>`
- `space: Space<Window>`

`Space<Window>` is Smithay’s desktop-space abstraction for mapped windows and outputs.

### WM / Runtime State

- `model: WmModel`
- `config`
- `config_paths`
- `managed_windows`
- `scene`
- `titlebar_overlays`
- `titlebar_layout`

This is where the compositor’s user-facing behavior is tied to the WM/runtime model.

### Event / Loop State

- `event_loop`
- `loop_signal`
- `socket_name`
- `ipc_server`
- `ipc_clients`
- `ipc_socket_path`

## Bootstrap Flow

Main bootstrap file:

- `apps/spiders-wm/src/app/bootstrap.rs`

### `build_state(...)`

This constructs the long-lived compositor state.

Important work done here:

- create Smithay compositor/xdg/shm/dmabuf/data-device states
- create seat and attach keyboard/pointer
- initialize Wayland socket listener
- initialize `WmModel`
- create `WmRuntime` with a `NoopHost`
- ensure default workspace and seat signal in the runtime model
- load config through authored-layout/config runtime
- initialize IPC listener
- build `SceneLayoutState`

Key point:

- runtime/model state exists before windows appear
- Smithay protocol state and WM/runtime state are bootstrapped in parallel and then tied together by the app

### `init_winit(...)`

This creates the nested backend and output.

Important work done here:

- initialize `WinitGraphicsBackend<GlesRenderer>`
- configure SHM / dmabuf formats
- create a Smithay `Output`
- map that output into `space`
- notify runtime via `WmSignal::OutputSynced`
- install winit event source into calloop

Important event handling path:

- `Resized`:
  - update output mode
  - emit `WmSignal::OutputSynced`
  - sync layout defaults
  - broadcast runtime events
  - schedule relayout
- `Input`:
  - route to `process_input_event(...)`
- `Redraw`:
  - route to `render_output_frame(...)`

## Smithay Handler Model In This Repo

Smithay requires the compositor state type to implement various protocol/desktop handlers.

These live under:

- `apps/spiders-wm/src/handlers/`
- `apps/spiders-wm/src/compositor/`

Important idea:

- Smithay delivers protocol events and desktop lifecycle callbacks
- `SpidersWm` handlers translate them into state changes in:
  - Smithay desktop state
  - runtime model state
  - scene/layout state

This is the core compositor boundary.

## Runtime / Command Flow

Main files:

- `apps/spiders-wm/src/runtime/command.rs`
- `apps/spiders-wm/src/runtime/mod.rs`

### Current flow

1. some input, IPC request, or higher-level action yields `WmCommand`
2. `SpidersWm::execute_wm_command_with_serial(...)` calls:
   - `dispatch_wm_command(self, command)`
3. `spiders-wm-runtime` translates command into `WmHostEffect`
4. `SpidersWm` implements `WmHost`
5. `SpidersWm::on_effect(...)` performs the real host-side behavior

Important point:

- in the real compositor, `WmHostEffect` is the important part
- `PreviewRenderAction` is irrelevant here and should be ignored

That is why returning `PreviewRenderAction::None` from the Smithay host implementation is correct.

### Why this matters

`spiders-wm` is not a preview consumer.

It should:

- execute host effects
- schedule compositor work
- not adopt preview-specific render behavior

## What `SpidersWm::on_effect(...)` Does

Examples in `apps/spiders-wm/src/runtime/command.rs`:

- `SpawnCommand`
  - spawns a shell command with compositor socket env
- `RequestQuit`
  - stops the loop signal
- `ActivateWorkspace`
  - selects or ensures target workspace
- `AssignFocusedWindowToWorkspace`
  - moves/toggles focused window workspace assignment
- `FocusWindow`
  - moves compositor focus
- `CloseFocusedWindow`
  - closes focused window
- `ReloadConfig`
  - reloads authored config and updates runtime defaults
- `SetLayout` / `CycleLayout`
  - mutate runtime layout selection
  - broadcast runtime events
  - schedule relayout

This is the real-host side of the command system.

## WM Model vs Smithay Desktop State

Two related but different things are maintained:

### Runtime / WM model

- `WmModel`
- used by `WmRuntime`
- tracks workspaces, focused windows, layout selection, assignments, etc.

### Smithay desktop state

- `Space<Window>`
- `PopupManager`
- `focused_surface`
- actual mapped `Window` objects and protocol surfaces

The application layer is responsible for keeping them coherent.

That is why compositor changes must be careful: a bug can leave runtime state and Smithay desktop state out of sync.

## Window Lifecycle In Practice

Relevant areas:

- `handlers/xdg_shell.rs`
- `compositor/windows.rs`
- `compositor/apply.rs`
- `actions/window.rs`

High-level lifecycle:

1. XDG toplevel surface appears
2. app creates/tracks a `ManagedWindow`
3. runtime/model gets a corresponding `WindowId`
4. window becomes mapped in Smithay `Space`
5. relayout / scene application positions it
6. focus and frame-sync state are updated as commits/configures happen
7. close/unmap destroys or removes that mapping and model state

## Input / Seat / Focus Flow

Relevant areas:

- `actions/seat.rs`
- `actions/focus.rs`
- `compositor/input.rs`
- `compositor/navigation.rs`

Important concepts:

- Smithay seat/keyboard/pointer are real protocol/input objects
- compositor focus is not just runtime focus; it must also update the actual focused surface
- moving focus usually requires both:
  - runtime/model update
  - Smithay seat/surface focus update

This is one of the easiest places to break behavior if runtime-side changes are applied blindly.

## Relayout / Scene Application Flow

Relevant areas:

- `compositor/layout.rs`
- `scene/adapter.rs`
- `scene/mod.rs`
- `compositor/apply.rs`

High-level idea:

1. runtime/model/config decide the current layout and assignments
2. scene/layout pipeline computes geometry and snapshot output
3. compositor applies that to mapped windows and titlebar overlays
4. redraw is scheduled

This is where runtime state becomes actual compositor geometry.

## Rendering / Frame Flow

Relevant areas:

- `compositor/rendering.rs`
- `frame_sync/`

High-level flow:

1. backend signals redraw
2. app renders current output frame
3. damage tracking is used
4. frame sync state tracks which surfaces should receive frame callbacks

If behavior looks visually wrong but runtime state looks correct, this is one of the first areas to inspect.

## Config Flow

Config load path:

- `app/bootstrap.rs::load_wm_config(...)`
- authored runtime: `JavaScriptNativeRuntimeProvider`

Reload flow:

- `app/lifecycle.rs::reload_config(...)`

High-level reload behavior:

1. reload config from authored runtime
2. update config paths and stored config
3. update scene config paths
4. sync runtime layout selection defaults
5. broadcast runtime events
6. schedule relayout

Important point:

- config reload is not just file parsing; it also changes runtime/default layout behavior and scene inputs

## IPC Flow

Relevant areas:

- `ipc.rs`
- `state.rs`

The compositor exposes an IPC socket and stores client streams in `ipc_clients`.

This is a likely future home for structured debug/dump commands.

## Invariants To Preserve

When changing compositor/runtime integration, preserve these invariants:

1. runtime model state and Smithay desktop state must stay coherent
2. focus changes must update both runtime meaning and actual seat/surface focus
3. layout changes must schedule relayout when compositor geometry changes
4. config reload must update both runtime defaults and scene/config inputs
5. preview-only concepts must not leak into the real Smithay compositor path

## Practical Guidance For Future Fixes

When a bug appears in `spiders-wm`, check which layer it belongs to first:

### If it is protocol or lifecycle related

Look at:

- `handlers/`
- Smithay state objects
- configure/commit/map/unmap flow

### If it is command / workspace / focus policy related

Look at:

- `runtime/command.rs`
- `actions/`
- `WmRuntime` / `WmModel`

### If it is geometry / placement / titlebar related

Look at:

- `scene/adapter.rs`
- `compositor/layout.rs`
- `compositor/apply.rs`

### If it is draw/frame behavior related

Look at:

- `compositor/rendering.rs`
- `frame_sync/`

## Current Architectural Conclusion

The real compositor host should remain thin in one specific sense:

- it should not own WM policy that already exists in `spiders-core` or `spiders-wm-runtime`

But it cannot be “dumb” in the same way as the web preview app, because Smithay requires it to be the concrete executor of compositor-side effects.

So the right model is:

- `spiders-core` / `spiders-wm-runtime`: brains for WM behavior
- `apps/spiders-wm`: concrete Smithay host/executor

That distinction is important when applying future refactors across both apps.
