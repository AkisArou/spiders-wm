# Real Session Wayland Compositor Plan

## Goal

Turn `apps/spiders-wm` from a nested `winit` compositor into a compositor that can run as the real Wayland session on a Linux desktop.

The target is not a demo DRM branch.

The target is a usable session compositor with:

- DRM/KMS output management
- GBM/EGL rendering on real devices
- libseat/seatd session handling
- real input devices on a real seat
- VT switching and session pause/resume
- multi-output support
- a protocol surface that is sufficient for normal desktop use
- a nested backend kept for development and CI

## Current State

Today `spiders-wm`:

- always boots the `winit` backend from `apps/spiders-wm/src/main.rs`
- initializes a single nested output in `app::bootstrap::init_winit`
- has no tty/session backend boot path
- has no real output hotplug path
- has no real libinput device handling
- has no layer-shell support
- has no data-control, fractional-scale, screencopy, output-management, idle, session-lock, or xdg-activation surface beyond the small subset already added

This means it is currently a nested development compositor, not a real session compositor.

## Non-Goals

These are explicitly out of scope for the first real-session milestone:

- X11 window manager mode
- perfect multi-GPU support on day one
- advanced color management / HDR
- full screen-casting portal stack
- tablet/pad feature completeness
- a polished lock screen UI
- shipping a full desktop shell

These may be added later, but they are not allowed to delay the first real-session bring-up.

## Definition Of Done

This plan is complete when all of the following are true:

1. `spiders-wm --backend tty` can start on a real VT.
2. It acquires session control through `libseat` or `seatd` without running as root.
3. It renders to at least one physical output through DRM/KMS + GBM/EGL.
4. It handles hotplug and mode changes for multiple outputs.
5. It receives keyboard and pointer input from libinput devices.
6. It survives VT switch away / back, including device pause/resume.
7. It can launch and manage normal Wayland applications on the real session.
8. It still supports nested `winit` mode for development.
9. The protocol surface is sufficient for common desktop apps and desktop components.
10. There is a documented launch path for a user session and a documented debugging path for nested mode.

## Required Architecture Direction

`spiders-wm` must stop treating backend startup as a single `winit` function.

We need an explicit backend abstraction with at least these runtime variants:

- `WinitBackend`
- `TtyBackend`

The compositor state should stay shared.

Backend-specific concerns should be isolated behind backend modules for:

- startup
- device/session lifecycle
- output lifecycle
- render scheduling
- input event ingestion

The existing WM/runtime/scene/compositor logic should remain backend-agnostic wherever possible.

## Reference Baseline

`~/projects/niri` is a useful reference for the scope of a real compositor.

Relevant categories visible there:

- session / tty backend
- drm lease
- dmabuf
- fractional scale
- idle inhibit / idle notify
- input method / text input
- keyboard shortcuts inhibit
- pointer constraints / relative pointer / pointer gestures
- primary selection / data control
- session lock
- xdg activation
- layer shell
- foreign toplevel
- output management
- screencopy
- gamma control
- ext workspace
- virtual keyboard / virtual pointer
- security context
- kde server decoration
- xdg foreign
- presentation
- cursor shape
- viewporter
- tablet manager

We do not need to copy `niri` exactly, but we should use it as a reality check for what a "real compositor" must eventually support.

When a protocol or backend area already exists in `~/projects/niri`, we should treat `niri` as the first implementation reference before inventing our own structure.

## Required Protocol Set

The list below is grouped by priority.

### Tier 0: Mandatory For First Real Session

These are not optional.

- `wl_compositor`
- `wl_subcompositor`
- `wl_shm`
- `xdg_wm_base`
- `zxdg_decoration_manager_v1`
- `wl_seat`
- `wl_output`
- `xdg-output`
- `linux-dmabuf`
- `wp_viewporter`
- `wp_presentation`
- `wp_cursor_shape_manager_v1`

### Tier 1: Required For Normal Desktop Use Soon After Bring-Up

- `zwlr_layer_shell_v1`
- `xdg_activation_v1`
- `wp_fractional_scale_manager_v1`
- `zwp_relative_pointer_manager_v1`
- `zwp_pointer_constraints_v1`
- `zwp_pointer_gestures_v1`
- `zwp_keyboard_shortcuts_inhibit_manager_v1`
- `zwp_primary_selection_device_manager_v1`
- `zwlr_data_control_manager_v1` or ext-data-control equivalent
- `zwp_text_input_manager_v3`
- `zwp_input_method_manager_v2`
- `zwp_virtual_keyboard_manager_v1`

### Tier 2: Strongly Recommended For A Practical Daily Driver

- `xdg_foreign`
- foreign toplevel management
- ext workspace management
- idle inhibit
- idle notify
- session lock
- screencopy
- output management
- gamma control
- single-pixel-buffer
- security-context

### Tier 3: Later, But Should Be Planned Up Front

- kde server decoration compatibility
- tablet manager
- drm lease
- output power management
- alpha modifier
- tearing control
- color management / HDR protocols
- xwayland integration
- desktop portal integration

## Implementation Plan

## Phase 1: Backend Refactor Foundation

### Deliverable

`spiders-wm` can choose between nested and tty backends at startup without duplicating compositor logic.

### Concrete tasks

1. Introduce a backend selection CLI/env surface.
   - Add `--backend winit|tty`
   - Add env override if needed, e.g. `SPIDERS_WM_BACKEND`
   - Keep `winit` as the default until tty is stable

2. Split current `init_winit()` path into backend modules.
   - `app/backend/winit.rs`
   - `app/backend/tty.rs`
   - shared helpers stay in `bootstrap.rs` only if truly shared

3. Introduce backend-owned runtime state.
   - backend enum in `SpidersWm`
   - output records and render state move out of the current single `WinitGraphicsBackend` field
   - separate backend-neutral output metadata from renderer/session internals

4. Make output handling support more than one output in the data model and render loop.
   - today the code effectively assumes a single output in several places
   - identify and remove `space.outputs().next()` assumptions

### Acceptance criteria

- `cargo run -p spiders-wm -- --backend winit` works exactly like today
- tty backend entry compiles, even if it still exits early with "not implemented"
- there is one clear place where backend startup is selected

## Phase 2: Session And TTY Control

### Deliverable

The compositor can start on a real VT and own the session through `libseat`/`seatd`.

### Concrete tasks

1. Add session backend dependencies and integration.
   - use `libseat` through Smithay session helpers if available in this tree
   - support `seatd`
   - do not require root for normal use

2. Implement tty acquisition.
   - open a free VT or use the current one when started from a login/session manager
   - set graphics mode where required
   - own session lifecycle cleanly

3. Implement pause/resume handling.
   - session pause must stop input/output device usage
   - session resume must re-enable outputs and rendering

4. Implement VT switching.
   - switch away cleanly
   - restore on return
   - do not leave DRM master/session ownership in a bad state

### Acceptance criteria

- compositor starts from a tty under `seatd`/`libseat`
- VT switch away/back works without panic or black permanent failure
- logs clearly show session active/paused/resumed transitions

## Phase 3: DRM/KMS + GBM/EGL Bring-Up

### Deliverable

One real monitor can display the compositor output through DRM/KMS.

### Concrete tasks

1. Enumerate DRM devices and pick a primary GPU.
   - use udev/device enumeration
   - distinguish render node vs card node

2. Create GBM/EGL renderer path for tty backend.
   - initialize GBM device
   - initialize EGL/GLES renderer
   - wire dmabuf feedback from real render node

3. Implement KMS output setup.
   - detect connectors, crtcs, planes
   - pick initial mode
   - create output objects in the WM state

4. Implement frame rendering + page flip scheduling.
   - damage tracker per output
   - submit frames to KMS
   - handle frame callbacks/presentation timing from real outputs

5. Preserve nested backend path.
   - no tty code should break `winit`

### Acceptance criteria

- tty backend can show a compositor background on one monitor
- a Wayland terminal opens and renders on the real output
- redraw loop is stable enough for interactive testing

## Phase 4: Real Input Stack

### Deliverable

Keyboard and pointer input come from libinput on the real seat.

### Concrete tasks

1. Add libinput event source.
   - keyboard, pointer, touch entry points
   - hook into existing command/focus pipeline

2. Seat naming and seat lifecycle.
   - use real seat names like `seat0`
   - manage device add/remove

3. Keyboard setup.
   - xkb config from config file
   - repeat rate/delay
   - modifier tracking

4. Pointer setup.
   - motion, buttons, axis
   - cursor rendering on real outputs
   - hotspot updates and cursor theme loading

5. Device hotplug handling.
   - new device attach
   - device removal

### Acceptance criteria

- keyboard focus and shortcuts work on tty backend
- pointer moves, clicks, and scrolls work on tty backend
- hotplugging a mouse/keyboard does not require compositor restart

## Phase 5: Multi-Output And Output Lifecycle

### Deliverable

The compositor supports multiple monitors with hotplug and mode changes.

### Concrete tasks

1. Remove single-output assumptions.
   - audit current `current_output_*` helpers
   - replace "first output" behavior with output-aware selection

2. Implement output hotplug.
   - connector add/remove
   - map/unmap outputs in compositor state

3. Implement output configuration policy.
   - initial arrangement
   - output positions
   - scale and transform basics
   - output enable/disable

4. Route workspaces to outputs.
   - workspace visibility per output
   - focused workspace per output
   - sensible default output assignment for new windows

5. Add output damage/render scheduling per output.

### Acceptance criteria

- plugging/unplugging monitors does not crash the compositor
- windows remain usable after output changes
- at least mirrored or side-by-side output arrangements work

## Phase 6: Protocol Completion For Real Use

### Deliverable

Protocol surface is good enough for launchers, bars, portals, modern apps, screenshots, and desktop components.

### Concrete tasks

1. Implement `zwlr_layer_shell_v1`.
   - panels, backgrounds, notifications, launchers
   - anchor/exclusive zone handling
   - this is mandatory very early for a practical desktop

2. Implement `xdg_activation_v1`.
   - app launch/focus requests
   - terminal-to-app activation flow

3. Implement fractional scale + viewporter completeness.
   - output scale updates
   - fractional scale negotiation

4. Implement pointer utility protocols.
   - relative pointer
   - pointer constraints
   - pointer gestures
   - cursor shape manager

5. Implement data/control protocols.
   - primary selection
   - data control for clipboard managers and portals

6. Implement text-input stack.
   - text-input-v3
   - input-method-v2
   - virtual keyboard manager

7. Implement foreign toplevel + workspace protocols.
   - app switchers
   - taskbars
   - workspace-aware shells

8. Implement screencopy and output management.
   - screenshots
   - future screen share integration
   - monitor configuration tooling

9. Implement idle inhibit / idle notify / session lock.
   - media players and presentations
   - lock/suspend flows

10. Defer non-blocking compatibility protocols.
   - KDE server decoration is optional compatibility, not a first-session blocker
   - tablet manager support is useful, but not required for first daily-driver viability

### Acceptance criteria

- a bar/launcher using layer-shell works
- clipboard managers work
- screenshots work
- normal app activation behavior works
- IME/text-input capable apps can function

## Phase 7: XWayland And Desktop Integration

### Deliverable

The compositor is practical on a normal Linux desktop, not just for pure Wayland apps.

### Concrete tasks

1. Add XWayland integration.
   - startup and lifecycle
   - X11 toplevel mapping
   - clipboard/focus basics

2. Integrate with desktop services.
   - session environment export
   - portal expectations
   - user service startup docs

3. Add a session launch entry.
   - `.desktop` session file or equivalent startup instructions
   - documented `seatd`/systemd-user requirements

### Acceptance criteria

- common XWayland apps launch and are usable
- the compositor can be selected as a session from a display manager or tty startup recipe

## Phase 8: Hardening And Release Criteria

### Deliverable

The tty backend is stable enough to use outside development.

### Concrete tasks

1. Add backend-specific smoke tests and manual test scripts.
   - nested smoke remains mandatory
   - add tty manual verification checklist

2. Add crash-resilience and cleanup checks.
   - session/device release on exit
   - no stale DRM master/session lockups

3. Add performance instrumentation for tty backend.
   - frame timing
   - missed page flips
   - render latency

4. Add docs.
   - how to run nested
   - how to run on tty
   - seatd/systemd-user requirements
   - known limitations

### Acceptance criteria

- normal startup/shutdown does not leave the tty or display stack broken
- manual checklist passes on at least one real hardware setup
- nested mode still works after tty changes

## Concrete File/Module Work

This is the expected implementation shape, not a suggestion.

### New module areas

- `apps/spiders-wm/src/backend/mod.rs`
- `apps/spiders-wm/src/backend/winit.rs`
- `apps/spiders-wm/src/backend/tty.rs`
- `apps/spiders-wm/src/backend/drm.rs`
- `apps/spiders-wm/src/backend/input.rs`
- `apps/spiders-wm/src/backend/session.rs`
- `apps/spiders-wm/src/backend/output.rs`

### Existing modules that will need major changes

- `apps/spiders-wm/src/main.rs`
- `apps/spiders-wm/src/app/bootstrap.rs`
- `apps/spiders-wm/src/state.rs`
- `apps/spiders-wm/src/compositor/layout.rs`
- `apps/spiders-wm/src/compositor/lookup.rs`
- `apps/spiders-wm/src/compositor/windows.rs`
- `apps/spiders-wm/src/handlers/mod.rs`
- `apps/spiders-wm/src/handlers/xdg_shell.rs`

### Protocol modules that should be added

- `apps/spiders-wm/src/handlers/layer_shell.rs`
- `apps/spiders-wm/src/handlers/xdg_activation.rs`
- `apps/spiders-wm/src/handlers/fractional_scale.rs`
- `apps/spiders-wm/src/handlers/pointer_constraints.rs`
- `apps/spiders-wm/src/handlers/relative_pointer.rs`
- `apps/spiders-wm/src/handlers/pointer_gestures.rs`
- `apps/spiders-wm/src/handlers/data_control.rs`
- `apps/spiders-wm/src/handlers/primary_selection.rs`
- `apps/spiders-wm/src/handlers/text_input.rs`
- `apps/spiders-wm/src/handlers/input_method.rs`
- `apps/spiders-wm/src/handlers/virtual_keyboard.rs`
- `apps/spiders-wm/src/handlers/screencopy.rs`
- `apps/spiders-wm/src/handlers/output_management.rs`
- `apps/spiders-wm/src/handlers/foreign_toplevel.rs`
- `apps/spiders-wm/src/handlers/xdg_foreign.rs`
- `apps/spiders-wm/src/handlers/ext_workspace.rs`
- `apps/spiders-wm/src/handlers/idle.rs`
- `apps/spiders-wm/src/handlers/session_lock.rs`

### Protocol modules that are explicitly later

- `apps/spiders-wm/src/handlers/kde_decoration.rs`
- `apps/spiders-wm/src/handlers/tablet.rs`

## Execution Order

There is one recommended order.

1. Backend refactor foundation
2. Session/tty control
3. DRM/KMS + GBM/EGL single-output bring-up
4. Real input stack
5. Multi-output support
6. Layer-shell and activation
7. Remaining core desktop protocols
8. XWayland and desktop integration
9. Hardening

Do not start with protocol breadth before tty rendering/input works.

Do not start with XWayland before the native Wayland session path is stable.

## Immediate Next Implementation Step

The first implementation PR after agreeing to this plan should do exactly this:

1. Introduce backend selection in `main.rs`
2. Move the current `winit` startup into a dedicated backend module
3. Introduce a `tty` backend module stub with a compileable startup path
4. Refactor `SpidersWm` state so backend-specific fields are no longer hard-coded to only `WinitGraphicsBackend`

If that first refactor is not done cleanly, every later tty/DRM step will be harder.

## Open Decisions To Lock Before Implementation

These should be decided explicitly before coding Phase 2.

1. Session stack
   - prefer Smithay session helpers over direct libseat integration unless we hit a concrete limitation

2. Launch model
   - support both tty startup and display-manager session startup

3. XWayland timing
   - defer until after native Wayland tty flow is stable

4. Protocol priority
   - `layer_shell`, `xdg_activation`, `fractional_scale`, `data_control`, and `screencopy` should be treated as early protocols, not "nice to have"
   - KDE server decoration and tablet manager are not required to reach the first real-session milestone

## Recommendation

Approve this plan only if we are willing to do the tty/session/backend refactor first.

Anything that tries to "just add DRM quickly" without restructuring backend ownership will produce a fragile compositor and slow down every later protocol and output task.

For implementation work, prefer to mirror proven patterns from `~/projects/niri` for:

- tty/session startup
- DRM/KMS device and output lifecycle
- libinput event ingestion
- layer-shell
- screencopy
- output management
- activation and foreign toplevel protocols
