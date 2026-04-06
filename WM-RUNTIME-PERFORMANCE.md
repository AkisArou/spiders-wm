# WM Runtime Performance

This file tracks runtime performance investigation for preview interactions in spiders-wm.

## Goal

- Make preview WM interactions feel immediate under realistic window counts.
- Focus first on runtime costs for `focus`, `close`, `spawn`, `swap`, and `workspace` actions.
- Avoid speculative UI-only fixes until the runtime hot path is measured and understood.

## Current Symptoms

- In `apps/spiders-wm-www` preview, `master-stack` with roughly `10-12` windows feels slow when moving focus through the stack column.
- Repeated `Alt+j` and `Alt+k` inputs lag behind visible focus changes.
- Closing windows also feels slow.
- The lag is not limited to a single keyboard binding; it appears tied to the preview update pipeline used by WM actions.

## What We Know So Far

### Browser Trace

- Reproduced in MCP on the preview route with `11 windows`.
- A rapid `Alt+j` interaction produced approximately:
  - `INP: 251 ms`
  - `processing duration: 221 ms`
  - `presentation delay: 30 ms`
- This points to expensive processing during the interaction itself, not primarily input queuing.

### Instrumented Findings

- We added stage timing in the preview app and `spiders-wm-runtime` layout path.
- The first high-value fix was removing the duplicate second `apply_layout_source(...)` when authored layout evaluation returned an unchanged `Config` and `SourceLayoutNode`.
- That reduced focus and close interactions from roughly two full layout passes to one.
- Example after that fix:
  - focus at `~11` visible windows:
    - WM command application: `~3ms`
    - one layout pass: `~64-65ms`
    - async authored evaluation still runs, but second apply is skipped with `unchanged=true`
    - total: `~142ms`
  - close at `~10` visible windows:
    - WM command application: `~5ms`
    - one layout pass: `~63-65ms`
    - second apply skipped with `unchanged=true`
    - total: `~136ms`

### Additional Completed Optimizations

The following optimizations were implemented after the initial measurements.

#### 1. Explicit Preview Render Actions

- `spiders-wm-runtime` now returns explicit preview render actions rather than leaving the app to infer refresh behavior from old/new runtime state.
- `spiders-wm-www` now mostly acts as a dispatcher:
  - apply session mutation
  - execute returned render action
- This removed app-owned layout invalidation policy from the preview UI layer.

#### 2. Stop Recompiling Unchanged CSS

- The preview runtime path now reuses `spiders_scene::pipeline::SceneCache`.
- Unchanged stylesheet text is no longer recompiled on focus/close/spawn.
- In steady state:
  - `precompile_stylesheet` dropped to `0ms`
  - remaining cost moved to layout compute itself

#### 3. Stop Session-Wide Async Reevaluation On Focus

- Preview reevaluation is now driven by explicit actions/request signals rather than generic `session` reactivity.
- Focus actions no longer trigger async authored reevaluation by default.
- This materially reduced focus latency.

#### 4. Lower-Layer Authored Dependency Tracking

- The JS/browser authored-layout runtime now records which `LayoutEvaluationContext` fields are actually read.
- That dependency summary is threaded through the authoring-layout service and preview evaluation path.
- This moved dependency knowledge to the lower runtime/config layers instead of the app.

#### 5. Lower-Layer Authored Evaluation Cache

- `SourceBundleAuthoringLayoutService` now caches evaluated layouts by:
  - prepared artifact fingerprint
  - evaluation context fingerprint
- The preview app now keeps a persistent service instance alive across reevaluations so that cache can actually hit.

#### 6. Scene-Level Response Cache

- `spiders_scene::pipeline::SceneCache` now also caches final `SceneResponse` values by request fingerprint.
- This is the correct lower layer for exact scene-request reuse.

### What Helped Most

- Removing duplicate second layout application
- Removing session-wide async reevaluation on focus
- Reusing compiled stylesheets

These reduced user-visible lag significantly for focus changes.

### What Did Not Materially Help Close

- Lower-layer authored evaluation cache
- Lower-layer final scene-response cache

Reason:

- `master-stack` legitimately depends on `windowCount`
- close changes the visible window count and stack distribution
- the resulting authored context and scene request are genuinely different
- so the caches do not get useful hits on the tested close transition

### Current Measured State

After all completed work:

- Focus:
  - no immediate runtime layout recompute
  - no async authored reevaluation by default
  - mostly reduced to WM state update cost plus lightweight bookkeeping
- Close:
  - command application: `~4-5ms`
  - immediate layout recompute: `~58-60ms`
  - reevaluate leg: still roughly `~137ms` total in the tested path

### Current Bottleneck

The remaining close bottleneck is now genuinely inside runtime/scene layout compute:

- `wm-runtime.layout.compute_from_cached_sheet`
- which still performs:
  - styled tree rebuild
  - Taffy tree rebuild and layout computation
  - snapshot rebuild

The problem is no longer primarily:

- CSS recompilation
- duplicate preview apply
- app-owned refresh policy
- whole-session reevaluation noise
- missing authored-layout caches

### Confirmed CSS / Layout Pipeline Finding

- `spiders-wm-runtime::compute_layout_preview_from_source_layout(...)` currently calls:
  - `compile_stylesheet(stylesheet_source)`
  - then `compute_layout_from_sheet(...)`
- That means unchanged CSS is recompiled on every preview recompute, including focus and close actions.
- This is not because the scene layer lacks a cache.
- `spiders_scene::pipeline::SceneCache` already exists and supports caching compiled stylesheets by layout/source.
- The preview runtime path is simply bypassing it and calling the stateless helpers directly.

Implication:

- CSS compilation should move to a cache-aware path.
- Recompilation should only happen when stylesheet source changes.
- In preview, that means on app start, config/style reload, or editor buffer changes that affect stylesheet source.
- It should not happen on focus-only or close-only interactions.

### Current Preview Hot Path

Observed flow for keyboard-driven WM commands in preview:

1. Keydown handler matches a binding.
2. `AppState.session.update(...)` applies the command to `PreviewSessionState`.
3. `PreviewSessionState::apply_command(...)` dispatches runtime WM behavior.
4. Preview state is updated through `spiders-wm-runtime`.
5. Preview layout is recomputed from layout source and stylesheet state.
6. Snapshot/geometries/styles are pushed back into the preview state.
7. The web view rerenders against the updated snapshot/session.

This means runtime work and web rendering work are currently tightly coupled.

### Read-Only Call Graph Findings

The current preview path is more expensive than it first appears because the same interaction crosses both a synchronous runtime path and an async layout-evaluation path.

#### Keyboard Focus Path In The Web App

Files:

- `apps/spiders-wm-www/src/main.rs`
- `apps/spiders-wm-www/src/app_state.rs`
- `apps/spiders-wm-www/src/session.rs`

Observed flow for `Alt+j` / `Alt+k`:

1. `install_keyboard_listener(...)` matches the binding.
2. `app_state.session.update(|state| state.apply_command(command))`
3. `PreviewSessionState::apply_command(...)`
4. `dispatch_runtime_wm_command(...)`
5. `PreviewSessionState::on_effect(...)`
6. `apply_host_runtime_command(...)`
7. `apply_runtime_preview_command(...)`
8. `crates/wm-runtime/src/session.rs::apply_preview_command(...)`
9. Rebuild a fresh `WmModel` from serialized preview state.
10. Perform focus/navigation/workspace logic.
11. Sync the resulting `WmModel` back into serialized preview state.
12. `app_state.refresh_preview_from_loaded_state()`
13. `PreviewSessionState::apply_layout_source(...)`
14. `compute_layout_preview_from_source_layout(...)`
15. Rebuild snapshot, diagnostics, unclaimed-window state, and geometries.

This is already a full round-trip for one focus action.

#### Async Preview Renderer Also Tracks Session Changes

File:

- `apps/spiders-wm-www/src/main.rs`

`install_preview_renderer(...)` depends on both:

- `app_state.editor_buffers.get()`
- `app_state.session.get()`

It builds a `PreviewRenderRequest` from the full session snapshot and editor buffers, then uses the formatted request as a cache key.

That means every session mutation can also trigger:

1. `PreviewRenderRequest::from_state(...)`
2. cloning the runtime state and buffers
3. async `evaluate_layout_source(...)`
4. config loading / layout service work
5. `app_state.apply_loaded_preview_layout(...)`
6. another `session.update(|state| state.apply_layout_source(...))`

So focus and close actions are not only paying the synchronous preview recompute path. They can also enqueue a second async layout-evaluation path because the renderer currently watches the entire session.

Update:

- We kept the async authored evaluation for correctness because layout code can depend on runtime state such as `focused_window_id`, `window.focused`, and `window_count`.
- We now skip the second expensive `apply_layout_source(...)` when authored evaluation returns unchanged config/layout output.
- This removed one redundant full layout pass, but the remaining single layout pass is still too expensive.

#### Runtime Command Path Rebuilds WmModel Every Time

Files:

- `crates/wm-runtime/src/session.rs`
- `crates/core/src/wm.rs`

For `apply_preview_command(...)`, `select_preview_workspace(...)`, and `set_preview_focused_window(...)`, the runtime currently:

1. normalizes the serialized preview session
2. constructs a fresh `WmModel`
3. inserts all workspaces
4. inserts all windows
5. restores focus memory
6. optionally rebuilds focus tree state from the snapshot
7. runs the WM operation
8. syncs the full model back into serialized preview state

This means repeated hot interactions do not operate on a long-lived in-memory WM model. They rebuild one from preview session data every time.

#### Directional Focus Rebuilds Candidate Structures Per Action

Files:

- `crates/wm-runtime/src/session.rs`
- `crates/core/src/navigation.rs`
- `crates/core/src/focus.rs`

For directional focus:

1. `focus_direction(...)`
2. `directional_target_window(...)`
3. `snapshot_window_geometry_candidates(...)`
4. `collect_preview_focus_tree_window_geometries(...)`
5. per-window `model.focus_scope_path(...)`
6. `select_directional_focus_candidate(...)`

Important details:

- Snapshot geometry candidates are rebuilt from the preview snapshot for every directional focus action.
- `preview_model_with_snapshot(...)` also rebuilds a `FocusTree` from snapshot geometry on every action.
- `select_directional_focus_candidate(...)` may repeatedly:
  - walk scope paths
  - regroup candidates into branches
  - infer split axes
  - sort branches
  - fall back to geometric candidate scoring

This is a plausible runtime cost center for repeated `Alt+j/k`.

#### Close Path Also Has Repeated Full-State Work

Files:

- `crates/wm-runtime/src/session.rs`
- `crates/core/src/focus.rs`
- `crates/core/src/wm.rs`

Closing the focused window currently does more than remove one item:

1. rebuild preview `WmModel`
2. compute preferred focus after focus loss
3. remove the window from the model
4. prune focus memory
5. sync the entire model back into preview session windows
6. recompute full preview layout/snapshot in the app layer

So the user-reported close lag is consistent with the architecture, not just with keyboard event handling.

#### Concrete O(n) / Repeated Work Already Identified

Files:

- `crates/core/src/wm.rs`
- `crates/core/src/focus.rs`
- `crates/wm-runtime/src/session.rs`
- `crates/wm-runtime/src/layout.rs`

Per-action costs visible from code include:

- `WmModel::set_window_focused(...)` loops over all windows to rewrite `focused` flags.
- `ordered_window_ids_on_current_workspace(...)` builds a fresh ordered vector by combining focus-tree order, hinted ids, and all model windows.
- `ordered_focusable_window_ids_on_current_workspace(...)` filters that vector again.
- `preferred_focus_window_on_current_workspace(...)` builds another filtered vector.
- `snapshot_window_geometry_candidates(...)` rebuilds geometry candidates from scratch.
- `FocusTree::from_window_geometries(...)` rebuilds focus-tree structure from scratch.
- `PreviewSessionState::sync_window_geometries_from_snapshot(...)` walks the snapshot tree and then loops visible windows again.
- `compute_layout_preview_from_source_layout(...)` revalidates layout, resolves windows, attaches titlebar content, compiles stylesheet, computes layout, and rebuilds the snapshot tree.

None of these are automatically bugs, but together they explain why runtime cost can climb quickly as visible windows increase.

## Runtime Areas In Scope

### 1. Preview Session Command Application

Files:

- `apps/spiders-wm-www/src/session.rs`
- `crates/wm-runtime/src/session.rs`
- `crates/wm-runtime/src/host.rs`

Questions:

- How much work does each command do before any layout recompute begins?
- Which commands clone or rebuild full state structures?
- Which operations scale with total windows vs visible windows?
- Which commands depend on `snapshot_root` only for geometry lookup versus for deeper semantic behavior?

### 2. Focus and Navigation

Files:

- `crates/wm-runtime/src/session.rs`
- relevant focus/navigation helpers in `crates/core`

Questions:

- How expensive are directional focus operations as window count grows?
- Does focus selection rebuild too much intermediate state per command?
- Are focus tree / geometry candidate structures recomputed from scratch on each action?
- Can the current preview snapshot be reused more efficiently for repeated focus moves?

### 3. Layout Recompute After WM Actions

Files:

- `apps/spiders-wm-www/src/app_state.rs`
- `apps/spiders-wm-www/src/session.rs`
- `crates/wm-runtime/src/layout.rs`
- `crates/wm-runtime/src/context.rs`

Questions:

- Which WM actions truly require a full layout recomputation?
- Which actions only change focus metadata but still trigger full layout/style/snapshot work?
- How much time is spent in:
  - `ValidatedLayoutTree::new(...)`
  - layout resolution
  - titlebar attachment
  - stylesheet compilation
  - `compute_layout_from_sheet(...)`
  - snapshot/geometries rebuild
- Are we recompiling unchanged stylesheet/layout inputs too often?

### 4. State Shape and Cloning

Files:

- `apps/spiders-wm-www/src/session.rs`
- `crates/wm-runtime/src/session.rs`
- `crates/wm-runtime/src/layout.rs`

Questions:

- How often do we clone `RuntimePreviewSession`, window vectors, snapshot nodes, config, or layout trees during a single action?
- Are there avoidable full-tree traversals for geometry sync or titlebar/style lookup?
- Are there opportunities to separate stable data from action-local data so commands touch less memory?

## Likely Bottlenecks

These are hypotheses, not conclusions.

### A. Full Layout Recompute For Focus-Only Actions

- Focus actions appear to trigger full preview recomputation even when window geometry should remain mostly unchanged.
- This is a prime suspect for `Alt+j/k` lag.
- Read-only pass confirms that focus actions currently go through both immediate preview recompute and a session-driven async preview evaluation path.

Update:

- One redundant recompute has already been removed.
- The remaining issue is the mandatory immediate layout recompute path, which still recompiles CSS and recomputes layout geometry on every focus action.

### B. Rebuilding Focus Structures Per Action

- Directional focus currently derives candidates from snapshot geometry and focus tree data.
- If those are rebuilt from scratch on every action, repeated focus input may pay the same setup cost repeatedly.
- Read-only pass confirms this is currently happening.

### C. Recompiling Stable Style/Layout Inputs Too Often

- Layout source and CSS often do not change during focus or close actions.
- If the runtime still recompiles stylesheet/layout state for those actions, that is likely wasted work.
- Read-only pass confirms that the preview renderer effect is currently keyed by the whole session, not just source/config inputs.

Update:

- Instrumentation confirmed that unchanged stylesheet text is currently recompiled on every single preview layout recompute.
- The codebase already contains `spiders_scene::pipeline::SceneCache`, so the next fix should reuse that existing cache instead of adding another independent stylesheet cache.

### D. Extra Traversal After Layout

- After preview computation, we still walk the snapshot again to collect geometries and track unclaimed windows.
- This may add noticeable overhead at higher window counts.

### E. Rebuilding Serialized And Derived State Repeatedly

- The runtime currently converts serialized preview state into `WmModel`, performs an action, then serializes back.
- This keeps the preview pipeline simple, but it likely adds repeated cloning and full-state walks on every hot interaction.

### F. Full Styled Tree And Taffy Rebuild On Close

- Current measurements strongly suggest the remaining close-path bottleneck is the scene/layout engine rebuilding too much from scratch.
- The next meaningful work should target subtree reuse or partial recomputation, not more higher-layer invalidation heuristics.

## Non-Goals For This Document

- Do not start with Leptos rerender tuning.
- Do not assume browser rendering is the root cause without runtime evidence.
- Do not apply batching/debouncing hacks as the main strategy.
- Do not optimize blindly around one observed trace.

UI/reactivity analysis should happen later in `WEB-PERFORMANCE.md`, after runtime costs are understood.

## Investigation Plan

### Phase 1. Runtime Cost Map

Status: mostly complete

- Map the command path for:
  - `focus`
  - `close`
  - `spawn`
  - `swap`
  - `workspace switch`
- Mark where state mutation ends and full layout recomputation begins.
- Record which functions are pure state transitions and which trigger expensive recomputation.

Read-only conclusions so far:

- `focus` and `close` both cross a runtime model rebuild path and a preview recompute path.
- Directional focus depends on snapshot-derived geometry/focus structures that are rebuilt per action.
- The preview renderer effect is too broad for hot interactions because it watches the whole session.
- The runtime hot path already contains multiple visible O(n) scans and vector rebuilds before web rendering even starts.

### Phase 2. Measure Command Cost By Stage

Status: complete enough for next fix

- Add targeted timing instrumentation around runtime stages in preview mode.
- Measure per-action timings for:
  - command dispatch
  - preview state transition
  - layout context building
  - layout resolution
  - stylesheet compilation
  - layout compute
  - snapshot post-processing
- Capture measurements at different window counts, especially `3`, `8`, and `12` windows.

Measured conclusions so far:

- WM command application is not the primary bottleneck.
- Titlebar attachment is measurable but small relative to total layout cost.
- The dominant remaining cost is the single remaining layout pass, especially:
  - stylesheet compilation
  - styled tree / layout compute

### Phase 2a. Stop Recompiling Unchanged CSS

Status: completed

- Route preview layout computation through a cache-aware stylesheet path.
- Reuse existing `spiders_scene::pipeline::SceneCache` rather than adding a parallel ad hoc cache.
- Ensure cache invalidation happens when stylesheet source changes.
- Verify that focus and close actions no longer recompile unchanged CSS.
- Re-measure interaction timings after this change.

Result:

- completed
- steady-state stylesheet recompilation is eliminated
- this was necessary, but not sufficient for close-path latency

### Phase 2b. Remove App-Owned Refresh Inference

Status: completed

- runtime now returns explicit preview render actions
- preview reevaluation is no longer keyed off whole-session reactivity

### Phase 2c. Lower-Layer Dependency Tracking And Authored Evaluation Cache

Status: completed

- dependency tracking was added in the JS/browser authored runtime
- dependency summary is now propagated through lower layers
- authored evaluation caching was added and made persistent

Result:

- correct architectural direction
- useful groundwork for future invalidation decisions
- not a major win for the tested close transition

### Phase 2d. Scene-Level Exact Request Cache

Status: completed

- exact scene-request caching now exists in `spiders-scene`

Result:

- correct lower-layer cache
- not a major win for the tested close transition because the scene request genuinely changes

### Phase 3. Separate Necessary vs Unnecessary Recompute

Status: pending

- Determine which actions must recompute full layout.
- Determine which actions can reuse prior compiled data or prior snapshot data.
- Be explicit about correctness risks for any reuse strategy.

### Phase 4. Design Runtime-Facing Fixes

Status: pending

- Prefer structural fixes over batching hacks.
- Candidate categories:
  - cache stable compiled stylesheet/layout artifacts
  - avoid rebuilding focus/navigation structures unnecessarily
  - reduce full-session cloning on hot actions
  - split focus-only updates from geometry-affecting updates
  - reduce snapshot post-processing passes

Current ordering:

1. stop recompiling unchanged stylesheets
2. remove app-owned invalidation policy
3. add lower-layer dependency tracking and evaluation caching
4. measure again
5. target scene/runtime structural reuse for close-path recompute

Current next candidates:

- cache or reuse styled layout subtrees for unchanged branches
- reduce or avoid rebuilding the full Taffy tree when only one stack child is removed
- reuse snapshot/style data for unaffected subtrees
- investigate whether `ResolvedLayoutNode` subtree identity can support incremental scene recompute

### Phase 5. Verify With Browser Traces

Status: pending

- Re-run the same MCP traces after runtime changes.
- Confirm lower processing duration for repeated `Alt+j/k`.
- Confirm closing windows also improves.
- Only then move to web/reactivity-specific analysis.

## Candidate Measurement Points

These are good places to instrument first:

- `apps/spiders-wm-www/src/session.rs`
  - `PreviewSessionState::apply_command`
  - `PreviewSessionState::apply_layout_source`
  - `PreviewSessionState::apply_preview_computation`
  - `PreviewSessionState::sync_window_geometries_from_snapshot`
- `crates/wm-runtime/src/session.rs`
  - `apply_preview_command`
  - `set_preview_focused_window`
  - `focus_direction`
  - `directional_target_window`
  - `snapshot_window_geometry_candidates`
  - `focus_tree_from_preview_snapshot`
- `crates/wm-runtime/src/layout.rs`
  - `compute_layout_preview_from_source_layout`
  - `attach_titlebar_content`
  - `collect_claimed_window_ids`
  - `collect_snapshot_geometries`

## Exit Criteria

This document is complete enough to hand off to `WEB-PERFORMANCE.md` once:

- We can attribute most interaction time to named runtime stages.
- We know which stage dominates `focus` and `close` actions.
- We have a concrete runtime fix plan backed by measurements.
- We have verified whether runtime optimization materially improves the browser trace.

## Notes

- A previous `requestAnimationFrame` batching attempt in the web app was not sufficient and should not be treated as the root fix.
- The correct next step is measurement and cost attribution inside the runtime pipeline, not more UI-side guesswork.
