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

Its current internal shape is intentionally small:

- `frame_sync/mod.rs` is the façade and ownership boundary
- `frame_sync/transaction.rs` holds the transaction and pending-configure machinery
- `frame_sync/render_snapshot.rs` holds snapshot capture and render elements

That is the preferred shape for now. Do not proactively split `frame_sync` further unless a specific internal pressure appears. The goal of this phase is containment, not creating more submodules.

The main rule for upcoming work is:

- new layout systems should produce target layout decisions
- `frame_sync` should continue to decide how those decisions become visible without intermediate frames

## Current Problem

`spiders-wm2` has moved beyond the earliest proof-of-concept layout, but the structure is still transitional.

Current source layout already includes:

- `main.rs`
- `app/`
- `actions/`
- `compositor/`
- `frame_sync/`
- `model/`
- `runtime/`
- `scene/`
- `state.rs`
- `layout.rs`
- `ipc.rs`
- `handlers/`

This is workable for the proof of concept, but it is not a good base for integrating:

- `spiders-scene`
- `spiders-config`
- the JS runtime crates
- richer models/actions/policies similar to `spiders-wm`

The remaining structural problem is more specific now:

- too much bootstrap and composition-root logic still lives in `state.rs`
- the new `app/bootstrap.rs` boundary exists, but `state.rs` still owns too much application-side orchestration
- compositor application, runtime orchestration, and model-to-Smithay bridging are still mixed together in `state.rs`
- `scene/adapter.rs` now owns the authoring-layout service, scene cache, and scene-backed target computation, but bootstrap tiling still remains as the final fallback path
- `frame_sync` is in a good containment state, but the rest of wm2 still needs to be reorganized around it

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
    layout.rs
    input.rs
    rendering.rs
  model/
    mod.rs
    wm.rs
    workspace.rs
    output.rs
    seat.rs
    window.rs
  actions/
    mod.rs
    facade.rs
    focus.rs
    output.rs
    seat.rs
    workspace.rs
    window.rs
  frame_sync/
    mod.rs
    transaction.rs
    render_snapshot.rs
  scene/
    mod.rs
    layout.rs
    styling.rs
    animation.rs
    adapter.rs
  runtime/
    mod.rs
    command.rs
```

This does not need to be created all at once. It is the target shape after reconciling with the directories that already exist today.

Notably:

- `compositor/`, `model/`, `actions/`, and `runtime/` are already present and should be expanded rather than reintroduced
- `frame_sync/` should stay small and stable instead of being split into more files right now
- a future `app/` boundary is still desirable, but it should be introduced only when it is ready to absorb real bootstrap code from `state.rs` and `winit.rs`
- `app/` now exists and should continue absorbing composition-root responsibilities from `state.rs`

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

Current note:

- keep `frame_sync` as `mod.rs` plus the two internal implementation files unless a concrete maintenance problem forces a split

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

Runtime should also expose a small high-level WM command surface for external integrations.

That command surface should express user-facing intents such as:

- focus next or previous window
- select a workspace by name
- select next or previous workspace
- close the focused window
- launch configured programs

Important rule:

- config, JS, IPC, and shortcut layers should target these high-level WM commands instead of calling compositor shell methods directly

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

### Phase 1: Consolidate the current composition root

Goal: reduce `state.rs` into a composition root rather than a giant behavior file.

Status:

- partially complete
- `app/bootstrap.rs` now owns startup assembly, config discovery, Wayland listener setup, IPC listener setup, and winit initialization
- relayout planning/application now lives in `compositor/layout.rs` instead of `state.rs`
- `compositor/input.rs`, `compositor/rendering.rs`, and `compositor/shell.rs` already exist
- `model/`, `actions/`, and `runtime/` already exist and are no longer hypothetical
- the remaining work is to keep moving orchestration and apply-layer responsibilities out of `state.rs`

Move next:

- keep building out `app/` as the composition-root boundary
- split any remaining startup or lifecycle glue out of `state.rs` when it is not true owned state
- decide whether long-term backend setup belongs entirely in `app/` or in a thinner Smithay/platform module under `compositor/`
- keep `state.rs` focused on owned state, lookups, and high-level relayout/application entry points

Do not change behavior in this phase.

### Phase 2: Introduce a real `model/`

Status:

- underway
- `model/` exists and already owns meaningful WM data

Create wm2 model types for:

- window record
- workspace record
- output record
- seat record
- overall wm model

Initially these can coexist with current Smithay-owned records.

The main purpose is to continue reducing reliance on the compositor object graph as the only source of truth.

### Phase 3: Introduce `actions/`

Status:

- underway
- `actions/` exists and already drives part of the runtime command surface

Extract pure operations from stateful methods such as:

- focus changes
- close requests
- workspace switching
- future move/floating policy

These should work on the wm2 model, not directly on Smithay objects.

### Phase 4: Add `runtime/` integration boundary

Status:

- underway
- `runtime/` exists, but the integration boundary is still thin and still wired too directly through compositor state

Prepare for:

- `spiders-config`
- JS runtime integration
- prepared layout artifacts
- reloadable configuration

This phase should produce a small service layer that can later feed scene/layout inputs into the compositor.

### Phase 5: Add `scene/adapter`

Status:

- underway
- `scene/adapter.rs` now owns wm2's scene-facing layout engine, including prepared-layout evaluation and `spiders-scene` target extraction
- bootstrap tiling remains behind that adapter as a last-resort fallback when scene evaluation or coverage fails

Create a wm2-specific adapter that converts:

- WM model state
- config/runtime outputs

into:

- `spiders-scene` layout inputs
- layout results that `state.rs` or a compositor apply layer can consume

### Phase 6: Replace temporary planner

Once `spiders-scene` is integrated:

- tighten scene coverage until the bootstrap planner is no longer needed
- delete the current temporary tiled planner logic
- keep `frame_sync` as the transition contract around the new layout outputs

## Recommended Near-Term Concrete File Moves

These are the safest next structural steps:

1. Create `crates/spiders-wm2/src/app/` and move `SpidersWm::new`, Wayland socket setup, config loading, and IPC/bootstrap helpers out of `state.rs`.
2. Reduce `state.rs` to owned state definitions, lookup helpers, relayout entry points, and thin application glue.
3. Decide whether any remaining backend/platform wiring should stay in `app/` or move under a thinner Smithay-facing compositor boundary.
4. Leave `frame_sync/` unchanged except for small API polish or internal test additions.
5. Continue moving model mutations and policy decisions toward `model/`, `actions/`, and `runtime/` instead of adding new behavior to `state.rs`.
6. Finish collapsing bootstrap fallback usage now that `scene/adapter.rs` can already produce real `spiders-scene` driven targets.

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
2. Continue the composition-root split by shrinking `state.rs` around the new `app/` bootstrap boundary.
3. Continue shrinking `state.rs` so model/runtime/compositor boundaries become explicit.
4. Strengthen wm2 `model/`, `actions/`, and `runtime/` around the already-existing modules.
5. Keep shrinking `state.rs` and runtime/compositor glue now that the scene boundary owns real target computation.
6. Remove the temporary planner once scene coverage is complete.

## Definition Of Success

The restructuring is successful when:

- `frame_sync` remains stable while layout logic changes around it
- `state.rs` becomes a small wiring module
- config/runtime-js integration is outside the Smithay event handlers
- scene/layout integration happens through an adapter instead of being smeared across compositor code
- wm2 gains clear model and action boundaries comparable to `spiders-wm`, but adapted for Smithay rather than river
