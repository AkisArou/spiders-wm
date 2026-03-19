# spiders-wm2 Feature Parity Checklist

This file tracks the clean-room rebuild of `spiders-wm` behavior in `spiders-wm2`.

Guidelines:
- Preserve behavior parity where it is intentional and documented.
- Prefer clean subsystem boundaries over copying old structure.
- Keep Smithay integration as an adapter layer, not the domain core.
- Treat this file as the local source of truth for rebuild progress.
- Integrate shared crates only through clean canonical models, never through adapter sludge.
- Make transactions the boundary between desired layout changes and committed scene updates.

## Current direction

The project is intentionally pivoting away from immediate scene mutation and toward a
transaction-based compositor architecture.

Agreed principles:
- `spiders-shared` is a candidate canonical boundary crate, but it must be re-evaluated and cleaned up where needed.
- `spiders-layout` should be integrated after the transaction pipeline exists, not before.
- `spiders-config` model/layout-selection pieces can be integrated before full JS runtime execution.
- `spiders-wm2` keeps Smithay/runtime/transaction internals local.

## Phase 1: Core model and crate boundaries

- [x] Split local model into dedicated `model/` modules.
- [x] Split pure reducers into dedicated `actions/` modules.
- [x] Split placement logic into dedicated `placement/` module.
- [x] Split Smithay-facing helper logic into `runtime_support/`.
- [x] Split backend event integrations into `backend/`.
- [x] Re-evaluate `spiders-shared` as the canonical shared vocabulary crate.
- [~] Decide which `spiders-shared` types are correct as-is, which need redesign, and which should stay local.
- [x] Replace local ids with canonical shared ids once the shared id design is approved.
- [ ] Remove duplicated local/shared state concepts instead of maintaining two parallel models.
- [ ] Keep Smithay objects out of model and shared boundary types.

## Phase 2: Transaction architecture

- [x] Introduce explicit desired scene vs committed scene state.
- [x] Add a local `transactions/` subsystem in `spiders-wm2`.
- [~] Track transaction participants per affected window/client.
- [~] Track pending configure serials and matching commits/acks.
- [~] Add transaction timeout/deadline handling so slow clients do not freeze the scene forever.
- [ ] Stop applying layout/action changes directly to Smithay `Space` from reducers/runtime refresh paths.
- [~] Commit only affected windows/subtrees, not the whole workspace/output blindly.
- [ ] Keep close/open/remove behavior transaction-aware to avoid visual artifacts.

## Phase 3: Efficient partial updates and rendering

- [ ] Track dirty layout scope at subtree/container/workspace level.
- [ ] Recompute only affected layout subtrees where possible.
- [ ] Reconfigure only windows whose effective geometry/state changed.
- [ ] Keep unrelated columns/groups/windows out of a transaction when unaffected.
- [ ] Use Smithay damage tracking as the render-layer optimization, not as a substitute for transaction planning.
- [ ] Track affected outputs and request redraw only where needed.
- [ ] Design the close-window-in-one-column case so only that column/subtree is recomputed and reconfigured.

## Phase 4: Compositor skeleton and runtime foundation

- [x] Bring up a minimal Smithay compositor loop.
- [x] Support outputs, seats, keyboard, and pointer.
- [x] Support xdg-shell surfaces and popups.
- [ ] Support layer-shell surfaces.
- [x] Track surface lifecycle in topology state.
- [x] Keep nested `winit` support behind an adapter boundary.
- [ ] Add workspace export only after basic compositor state is stable.

## Phase 5: Shared snapshot integration

- [~] Reconcile `spiders-wm2` local window/workspace/output state with `spiders_shared::wm` snapshots.
- [ ] Decide the canonical ordering source for windows/workspaces/outputs.
- [ ] Move only true cross-crate boundary types into `spiders-shared`.
- [ ] Keep transaction state, runtime bindings, scene state, and pointer interaction local to `spiders-wm2`.
- [~] Use shared `LayoutRect`/snapshot types at the model boundary and keep Smithay rectangles runtime-local.

## Phase 6: Layout runtime integration

- [ ] Integrate `spiders-layout` validation and resolution pipeline.
- [ ] Support the structural model: `workspace`, `group`, `window`, `slot`.
- [ ] Use `spiders-layout`/`taffy` for tiled geometry computation.
- [ ] Merge tiled layout results with local floating/fullscreen policy in placement.
- [ ] Cache and recompute layout only when config, topology, or WM state changes.
- [ ] Avoid rebuilding the entire world when only one subtree changes.

## Phase 7: Config integration

- [ ] Integrate `spiders-config::model::Config` and layout selection.
- [ ] Use config-driven workspace names and selected layouts.
- [ ] Integrate declarative keybinding config.
- [ ] Dispatch bindings into WM actions.
- [ ] Keep command spawning as a runtime side effect, not a pure reducer action.
- [ ] Defer full JS config runtime execution until transactions and shared model integration are stable.

## Phase 8: Window management parity

- [ ] Workspace activation and assignment across outputs.
- [x] Focus movement.
- [x] Focus-after-close behavior.
- [x] Move, swap, and close focused window.
- [ ] Send/focus monitor left-right.
- [x] Floating window support.
- [x] Floating geometry mutation.
- [x] Fullscreen toggle.
- [ ] Visible-window recomputation should be transaction-driven rather than immediate.

## Phase 9: IPC and shared API surface

- [ ] Add a Unix socket IPC server.
- [ ] Support snapshot/state queries.
- [ ] Support current workspace query.
- [ ] Support focused window query.
- [ ] Support current output query.
- [ ] Support monitor list and workspace name queries.
- [ ] Support action submission.
- [ ] Support subscriptions/event broadcast.
- [ ] Ensure IPC payloads come from the cleaned shared snapshot model.

## Phase 10: Effects, titlebars, transitions

- [ ] Parse effects CSS.
- [ ] Compute per-window effect state.
- [ ] Derive decoration policy from effects state.
- [ ] Add compositor-drawn titlebar planning.
- [ ] Add titlebar hit-testing and interaction wiring.
- [ ] Add open transitions.
- [ ] Add close transitions.
- [ ] Add resize transitions.
- [ ] Rebuild transitions around `keyframe` timelines/interpolation.
- [ ] Add workspace transitions only when documented pseudo-states are fully supported.

## Phase 11: Missing-but-desired parity

These are documented or expected features that are not fully implemented in the current `spiders-wm` crate and should be treated as explicit `spiders-wm2` milestones.

- [ ] `autostart`
- [ ] `autostart_once`
- [ ] window rules
- [ ] input config application
- [ ] output policy application
- [ ] config runtime JS API hooks
- [ ] full workspace transition CSS semantics

## Preserve

- [ ] Keep a typed config/layout/runtime boundary.
- [ ] Keep WM state separate from topology state.
- [ ] Keep IPC snapshot/action based.
- [ ] Keep effects and titlebars as derived runtime state.
- [ ] Keep Smithay as an adapter layer.
- [ ] Keep transactions as a local compositor concern, not a leaked config/layout concern.

## Do not copy literally

- [ ] Avoid carrying over bootstrap/scenario/transcript complexity unless it proves necessary.
- [ ] Avoid overgrown nested-`winit` orchestration.
- [ ] Avoid mixing backend-specific side effects into domain logic.
- [ ] Avoid direct immediate-apply layout updates that bypass transaction staging.
- [ ] Avoid maintaining duplicate local/shared models with patchy adapters between them.

## Immediate next milestones

- [~] Audit `spiders-shared` type-by-type and decide keep/change/remove.
- [x] Design `spiders-wm2` transaction manager API.
- [x] Replace local ids with shared ids after the audit is complete.
- [x] Introduce desired scene / committed scene snapshots.
- [ ] Integrate `spiders-layout` only after the transaction boundary exists.
- [ ] Integrate `spiders-config` model/layout selection before full JS runtime execution.

## Current implementation notes

- `spiders-wm2` now uses shared `WindowId`, `WorkspaceId`, and `OutputId` types from `spiders-shared`.
- Local WM state can emit `spiders_shared::wm::StateSnapshot` through `WmState::snapshot`.
- A first local `transactions` module now stages `desired` snapshots, diffs them against the last committed snapshot, and records affected windows/workspaces/outputs.
- Active-workspace refresh now applies only pending affected windows instead of blindly iterating every known window.
- Transaction diffing now treats visibility changes and fullscreen gating as dependency edges so newly hidden/revealed siblings are included in the affected set.
- Pending transactions now track per-window configure serials plus ack/commit progress, and committed snapshots advance only when all tracked participants are ready.
- Pending transactions now carry deadlines and can complete on timeout during redraw polling, so a stuck client no longer blocks commit forever.
- Pending transactions now build a refresh plan that can expand dirty `window` changes to full `workspace` or `output` scope before Smithay-side application.
- Pending transactions now carry an explicit dirty-scope hierarchy (`window`, `workspace`, `output`, `layout subtree`, `full scene`) so future layout integration has a transaction-native place to land.
- Refresh planning now includes a layout recompute plan (`workspace_roots` / `full_scene`) and transaction debug summaries, so future layout integration and diagnostics can attach without redesigning the transaction layer.
- `spiders-wm2` now has a local `layout` boundary with revisioned recompute summaries, and transaction diagnostics now include transaction ids for tracing multi-step scene changes.
- Tiled placement now reads from locally recomputed layout state instead of a single hard-coded tiled rectangle, giving the transaction/layout boundary a real geometry consumer.
- The first `spiders-layout` adapter now exists for tiled workspace geometry, replacing the previous pure placeholder column splitter with a real pipeline-backed layout pass.
- `spiders-wm2` layout recompute now reads config-selected layout stylesheets per workspace, creating the first real bridge from config layout selection into transaction-driven geometry.
- Workspace layout selection in `spiders-wm2` now prefers canonical config workspace ordering and per-monitor overrides before falling back to legacy numeric-name indexing.
- `spiders-wm2` now has an explicit local config runtime state with source/revision tracking, so layout recompute can depend on more than an unstructured default config blob.
- `spiders-wm2` now has a real config load/apply path: startup installs a built-in default config, optional external config paths can replace it with prepared/authored sources, and workspace catalogs are updated when config changes.
- `RefreshPlan.outputs` now feeds a local render plan so winit redraw can skip unchanged outputs instead of treating every frame as globally dirty.
- Config runtime can now store prepared/authored layout trees per layout name, and layout recompute prefers those installed trees over the fallback runtime tree shape when available.
- `spiders-wm2` now has an explicit handoff point for `PreparedLayoutEvaluation`, so a real layout runtime/service can install evaluated layout trees into config runtime without test-only plumbing.
- `spiders-wm2` now ships a local built-in layout service/runtime bridge that refreshes prepared layout artifacts during workspace refresh, exercising the service -> evaluation -> config runtime -> layout pipeline path end-to-end.
- `spiders-wm2` can now switch to the JS layout runtime when a config path is provided, and keyboard-triggered config reload (`Alt+Shift+R`) re-applies config plus layout artifacts through the same runtime path.
- `spiders-wm2` now exposes a simple control-socket command path (`reload-config`) when `SPIDERS_WM2_CONTROL_SOCKET` is configured, giving config reload a real IPC transport instead of only an internal method or keybinding.
- Control-socket commands now return responses/errors, and `dump-transaction` provides lightweight runtime inspection for transaction/config/render state.
- The control socket now accepts a JSON command envelope and supports more than reload (`switch-workspace`, `refresh-layout-artifacts`, `dump-transaction`), giving wm2 a reusable runtime command surface instead of a single special case.
- Query commands now expose structured runtime inspection (`list-outputs`, `list-workspaces`, `list-windows`), and the built-in fallback layout runtime is isolated behind a Cargo feature flag instead of being an unconditional architectural dependency.
- Winit redraw now flows through a reusable output render helper, and inspection payloads include focus/pending-transaction/render-dirty metadata instead of only bare lists.
- Layout state now tracks both desired and committed tiled geometry, and `dump-geometry` exposes both over the control socket for transaction-aware placement inspection.
- Layout inspection now includes desired/committed layout snapshots via `dump-layout-tree`, and the winit render path now iterates Smithay outputs through a reusable helper shape instead of a single hard-wired render block.
- Layout artifact provenance is now inspectable through the control socket (`dump-layout-artifacts`), including config source/revision, installed layout trees, and per-workspace effective layout selection/install status.
- Installed layout trees now carry runtime provenance (`BuiltIn` vs `JsRuntime`), and artifact inspection reports which runtime path produced the currently installed tree for each selected layout.
- Runtime/backend provenance is now inspectable through `dump-runtime`, and output inspection payloads now describe render capability/dirty-state metadata instead of only identity fields.
- Transaction inspection now includes recent commit history with ready-vs-timeout resolution, so runtime diagnostics can show not just the current pending transaction but how recent transactions actually settled.
- Transaction diagnostics now include timing metadata (pending age/deadline and historical commit durations), making timeout behavior inspectable instead of only inferable.
- Participant-level transaction state is now inspectable (waiting-for-ack vs waiting-for-commit vs ready), and timeout history records which window ids were still unresolved when a transaction had to settle anyway.
- Transaction history now also records superseded pending transactions, so inspection can distinguish "timed out" from "replaced by a newer desired scene" during rapid state churn.
- Committed geometry inspection now reads committed transaction snapshot windows/modes instead of only live WM state, so `dump-geometry` can still report the last committed scene during pending closes/mode flips.
- Workspace/window inspection now exposes desired vs committed views, and window unmap now defers final binding/state removal until the pending transaction commits.
- Dirty layout subtree planning now derives workspace roots from affected windows in both committed and desired snapshots instead of only explicit workspace diffs.
- Focus handoff now prefers the last committed focused surface while a transaction is pending, reducing focus/raise churn before the new scene commits.
- Pending refresh now also prefers committed visible-window state for show/hide decisions, and output inspection exposes desired, committed, and live runtime views side by side.
- Pending refresh now also keeps mapped window positions on committed geometry until commit, while still sending desired configure sizes ahead of the transaction boundary.
- Render dirtiness from pending refresh plans is now staged and only promoted on transaction commit, so desired scene prep no longer forces early output presentation.
- Transaction diagnostics now track coalescing root/depth metadata so rapid superseded updates can be inspected as a replacement chain instead of isolated history rows.
- Runtime inspection is moving to an explicit desired/committed/presented split; geometry, window, workspace, and output payloads now expose presented state directly instead of inferring it ad hoc.
- `dump-transaction` and `dump-layout-tree` now expose presented state alongside desired/committed views so transaction and layout diagnostics align with the same presentation model.
- Runtime payload builders for geometry and transaction inspection are now factored into testable helpers, making presented-state diagnostics easier to verify as the model evolves.
- Transaction participants now ignore stale/untracked ack+commit events more strictly, reset cleanly on reconfigure, and require all tracked participants to become ready before commit.
- Timeout diagnostics now distinguish stalled vs partially-ready pending transactions and record participant readiness counts in transaction history/inspection payloads.
- Scene application is still immediate after staging; smarter timeout policy and real subtree-scoped layout recomputation are the next transaction milestones.
