# spiders-wm2 Architecture Plan

## Goal

Restructure `spiders-wm2` so it is ready to absorb the existing project crates and higher-level behavior from `spiders-wm`, while preserving the current frame-sync behavior as a stable subsystem.

This plan is intentionally about structure first, not feature porting.

## Frame Sync Status

`frame_sync` is complete enough for the current phase.

That does not mean "done forever". It means:

- the core behavioral rules are in one module
- the public API is narrower and higher-level than before
- the module has targeted tests and smoke coverage
- the remaining code outside `frame_sync` is mostly Smithay plumbing or temporary layout application

For restructuring purposes, `frame_sync` should now be treated as a mostly frozen subsystem.

The main rule for upcoming work is:

- new layout systems should produce target layout decisions
- `frame_sync` should continue to decide how those decisions become visible without intermediate frames

## Current Problem

`spiders-wm2` still has an early-stage structure:

- `main.rs`
- `state.rs`
- `winit.rs`
- `input.rs`
- `handlers/`
- `frame_sync/`

This is workable for the proof of concept, but it is not a good base for integrating:

- `spiders-scene`
- `spiders-config`
- the JS runtime crates
- richer models/actions/policies similar to `spiders-wm`

Right now too much responsibility still lives in `state.rs`.

## Design Principles

1. Keep `frame_sync` independent from layout algorithm details.
2. Separate persistent WM model from Smithay object ownership.
3. Separate policy/actions from backend application.
4. Introduce adapter layers for external crates instead of leaking them through the compositor core.
5. Prefer modules that can be migrated incrementally without breaking current behavior.
6. Preserve a thin Smithay-facing shell that wires together well-bounded subsystems.

## Target Module Layout

Recommended medium-term structure for `crates/spiders-wm2/src`:

```text
src/
  main.rs
  app/
    mod.rs
    bootstrap.rs
    lifecycle.rs
  compositor/
    mod.rs
    shell.rs
    outputs.rs
    input.rs
    rendering.rs
    dmabuf.rs
    popups.rs
  model/
    mod.rs
    wm.rs
    workspace.rs
    output.rs
    seat.rs
    window.rs
  actions/
    mod.rs
    focus.rs
    workspace.rs
    window.rs
    layout.rs
  frame_sync/
    mod.rs
    runtime.rs
    transaction.rs
    snapshots.rs
    close_path.rs
    planner.rs
  scene/
    mod.rs
    layout.rs
    styling.rs
    animation.rs
    adapter.rs
  runtime/
    mod.rs
    config.rs
    js.rs
    registry.rs
  smithay/
    mod.rs
    state.rs
    handlers/
    winit.rs
```

This does not need to be created all at once. It is the target shape.

## Architectural Roles

### 1. `frame_sync`

Frozen transition subsystem.

Responsibilities:

- transactions and blockers
- configure commit matching
- snapshots and overlays
- relayout deferral policy
- frame callback eligibility during transitions
- transition render elements

Must not own:

- CSS layout computation
- config loading
- JS runtime concerns
- workspace/window policy unrelated to visibility timing

### 2. `model`

Pure WM state model, similar in spirit to `spiders-wm::model`, but adapted for Smithay.

Responsibilities:

- stable IDs for windows, outputs, seats, workspaces
- persistent workspace and focus state
- metadata about windows and outputs
- future scene/layout inputs

Should avoid direct Smithay types where practical, except in thin handle wrappers if truly needed.

Key point:

- `ManagedWindow` in its current form is too compositor-specific to be the long-term model

### 3. `actions`

State and policy operations independent of backend events.

Responsibilities:

- focus changes
- workspace activation
- moving windows between workspaces
- close policy
- layout invalidation requests

This is where `spiders-wm` already has a useful precedent.

### 4. `compositor` or `smithay`

The Smithay-facing shell.

Responsibilities:

- event loop integration
- Wayland globals and handlers
- `Space<Window>` and popup management
- renderer backend ownership
- input event translation
- applying model/action results to Smithay objects

This layer should become thinner over time.

### 5. `scene`

Integration boundary for `spiders-scene`.

Responsibilities:

- build scene/layout inputs from WM model
- compute desired layout and animation state
- expose render/layout results in wm2-friendly types

Important rule:

- `scene` decides _what layout should be_
- `frame_sync` decides _how layout transitions become visible atomically_

### 6. `runtime`

Integration boundary for config discovery, JS runtime, prepared layouts, registry/cache concerns.

Responsibilities:

- load and reload config
- call JS layout/runtime services
- cache prepared scene artifacts
- own integration with `spiders-config` and JS runtime crates

## What To Port From `spiders-wm`

Good ideas to reuse conceptually from `spiders-wm`:

- `model/` split
- `actions/` split
- distinct backend/application layer
- runtime/config/runtime-js integration living outside the low-level compositor loop

Things not to port directly:

- river-specific backend assumptions
- animation/render paths coupled to river limitations
- the current tiled layout algorithms as architecture-defining abstractions

## Immediate Restructure Plan

### Phase 1: Split `state.rs` by responsibility

Goal: reduce `state.rs` into a composition root rather than a giant behavior file.

Move first:

- socket/display/bootstrap setup into `app/bootstrap.rs`
- focus/window lifecycle helpers into `compositor/shell.rs`
- redraw/snapshot/render orchestration into `compositor/rendering.rs`
- input processing into `compositor/input.rs`

Do not change behavior in this phase.

### Phase 2: Introduce a real `model/`

Create wm2 model types for:

- window record
- workspace record
- output record
- seat record
- overall wm model

Initially these can coexist with current Smithay-owned records.

The main purpose is to stop using the compositor object graph as the only source of truth.

### Phase 3: Introduce `actions/`

Extract pure operations from stateful methods such as:

- focus changes
- close requests
- workspace switching
- future move/floating policy

These should work on the wm2 model, not directly on Smithay objects.

### Phase 4: Add `runtime/` integration boundary

Prepare for:

- `spiders-config`
- JS runtime integration
- prepared layout artifacts
- reloadable configuration

This phase should produce a small service layer that can later feed scene/layout inputs into the compositor.

### Phase 5: Add `scene/adapter`

Create a wm2-specific adapter that converts:

- WM model state
- config/runtime outputs

into:

- `spiders-scene` layout inputs
- layout results that `state.rs` or a compositor apply layer can consume

### Phase 6: Replace temporary planner

Once `spiders-scene` is integrated:

- delete the current temporary tiled planner logic
- keep `frame_sync` as the transition contract around the new layout outputs

## Recommended Near-Term Concrete File Moves

These are the safest next structural steps:

1. Create `crates/spiders-wm2/src/compositor/`.
2. Move redraw, frame dispatch, and output render code out of `state.rs` and `winit.rs` into `compositor/rendering.rs`.
3. Move focus and window lifecycle helpers out of `state.rs` into `compositor/shell.rs`.
4. Move input event translation into `compositor/input.rs`.
5. Leave `frame_sync/` unchanged except for small API polish.
6. Introduce `model/mod.rs` with minimal placeholder model structs before any scene integration.

That sequence improves structure without forcing layout migration early.

## Integration Boundaries With Other Crates

### `spiders-config`

Should enter through `runtime/config.rs`, not directly inside compositor handlers.

### JS runtime crates

Should enter through `runtime/js.rs` or a runtime service layer, not through `frame_sync` or Smithay handlers.

### `spiders-scene`

Should enter through `scene/adapter.rs`.

The adapter should translate wm2 model state into scene inputs and scene outputs back into layout targets.

### `spiders-shared`

Use as the shared typed interface for snapshots, layout/style payloads, and API-level state where it already fits.

## Risks To Avoid

1. Rebuilding `state.rs` inside a new directory structure without actually separating responsibilities.
2. Letting `frame_sync` absorb temporary tiling logic or CSS scene logic.
3. Binding the future wm2 model too tightly to Smithay internals.
4. Pulling config/runtime-js concerns directly into handlers.
5. Starting scene integration before the model and runtime boundaries exist.

## Suggested Order Of Execution

1. Freeze `frame_sync` behavior and API shape.
2. Split compositor code out of `state.rs`.
3. Introduce wm2 `model/`.
4. Introduce wm2 `actions/`.
5. Introduce `runtime/` service boundary.
6. Integrate `spiders-scene` behind `scene/adapter`.
7. Remove the temporary planner.

## Definition Of Success

The restructuring is successful when:

- `frame_sync` remains stable while layout logic changes around it
- `state.rs` becomes a small wiring module
- config/runtime-js integration is outside the Smithay event handlers
- scene/layout integration happens through an adapter instead of being smeared across compositor code
- wm2 gains clear model and action boundaries comparable to `spiders-wm`, but adapted for Smithay rather than river
