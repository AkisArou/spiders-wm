# Preview Runtime Extraction Plan

This document replaces the earlier idea of keeping `spiders-web-bindings` as a meaningful long-term layer.

For the concrete next-step API shape, see `docs/runtime-api-sketch.md`.

The current position is:

- `apps/spiders-wm-playground` is deprecated reference code only.
- `crates/spiders-web-bindings` should be treated as a temporary source of extractable code, not as architecture to preserve.
- `apps/spiders-wm` and `apps/spiders-wm-www` should become thinner over time.
- Shared semantics should live in `spiders-core` when they are truly cross-platform.
- Preview/session simulation should live in a dedicated crate, but not under `crates/runtimes/` because that directory is reserved for authored-language runtimes like JS/Lua/Python.

## Goals

1. Keep `apps/spiders-wm` thin as the real Wayland platform adapter.
2. Keep `apps/spiders-wm-www` thin as the web preview/editor shell.
3. Eliminate `crates/spiders-web-bindings` as an architectural dependency.
4. Remove hardcoded preview hacks such as the master-stack snapshot override path.
5. Deduplicate shared lifecycle/focus/session semantics without polluting `spiders-core` with browser or wasm glue.

## Final Intended Structure

Top-level crates/apps after the migration should look like this:

```text
apps/
  spiders-wm/
  spiders-wm-www/
  spiders-wm-playground/        # deprecated, eventual deletion

crates/
  runtimes/
    js/
    # later: lua/, python/, ...
  spiders-cli/
  spiders-config/
  spiders-core/
  spiders-css/
  spiders-ipc/
  spiders-logging/
  spiders-wm-runtime/
  spiders-scene/
  spiders-web-bindings/         # temporary extraction source, eventual deletion
  spiders-wm-river/
```

## Roles

### `spiders-core`

Role:

- Source of truth for cross-platform WM semantics.
- Owns lifecycle invariants, focus semantics, navigation semantics, and stable shared data contracts.

Should contain:

- Window lifecycle semantics.
- Focus eligibility and layout eligibility rules.
- Stable external DTOs for runtime/session consumers.
- Shared command/lifecycle tests.

Should not contain:

- wasm-bindgen glue.
- `JsValue` parsing.
- browser preview heuristics.
- compositor transaction waiting.

### `spiders-wm-runtime`

Role:

- Pure Rust shared runtime/orchestration crate for app adapters.
- Shared by `apps/spiders-wm`, `apps/spiders-wm-www`, and future adapters like `apps/spiders-wm-xorg`.

Should contain:

- Shared runtime orchestration used by thin app/platform shells.
- Config/runtime bootstrapping that can be reused across app adapters.
- Preview/session state reducer.
- Typed preview command application.
- Layout preview computation pipeline using `spiders-scene` and `spiders-core`.
- Projection between preview state and core model/snapshots.
- Web-preview lifecycle handling for open/close transitions.

Notes:

- The current crate name is acceptable for now, but if it grows into the main shared app runtime rather than just preview/session logic, we should consider renaming it later to better match its role.

Should not contain:

- React-specific code.
- wasm-bindgen exports.
- JS raw module loading policy.
- hardcoded layout-specific hacks.

### `apps/spiders-wm`

Role:

- Real Wayland compositor/platform adapter.
- Owns frame-sync transactions, protocol integration, snapshot overlays, and platform timing.

Should remain thin with respect to semantics:

- It should orchestrate platform events and transactions.
- It should consume shared semantics from `spiders-core`.
- It should consume shared runtime/orchestration from `spiders-wm-runtime`.
- It should not re-implement lifecycle/session/config orchestration that belongs in a reusable crate.

### `apps/spiders-wm-www`

Role:

- Web UI shell.
- Monaco/editor/Leptos integration.
- Pure presentation and local UI state.

Should remain thin with respect to semantics:

- It should call into `spiders-wm-runtime` directly.
- It should not own focus/lifecycle policy.
- It should not depend on `spiders-web-bindings` long-term.

### `spiders-config`

Role:

- Platform-neutral config schema and authored-config service substrate.
- Shared by app adapters and shared runtime code.

Should contain:

- Config data types.
- Authored/prepared config loading services.
- Path/value structs like `ConfigPaths` and `ConfigDiscoveryOptions` as plain data.

Should not contain:

- Linux/UNIX-specific discovery policy baked in as the only supported path.
- Browser-specific config loading policy.
- App bootstrapping/orchestration.

Direction:

- `spiders-wm-runtime` should orchestrate shared config/runtime bootstrapping on top of `spiders-config`.
- App adapters should provide platform-specific discovery and storage details.
- `apps/spiders-wm` can populate config discovery from env/XDG/HOME-like sources.
- `apps/spiders-wm-www` should adapt config loading to web/virtual workspace sources rather than pretending to be a UNIX filesystem.

### `crates/runtimes/js`

Role:

- Authored layout runtime for JS.
- Used by the real compositor and other tools that execute authored layouts.

Non-goal:

- It is not the place for preview session simulation.
- It is not the place for app bootstrapping/orchestration.

## Classification Of Current `spiders-web-bindings`

### Move toward `spiders-core`

These are semantics or contracts that are broader than the preview crate:

- Lifecycle invariants around `mapped`, `closing`, focusability, and layout eligibility.
- Stable external window/session DTOs where current duplication is unnecessary.
- Shared lifecycle tests that should hold for all platforms.

### Move into `spiders-wm-runtime`

These are real runtime behaviors, but preview/runtime-specific rather than core domain logic:

- Shared app/runtime bootstrapping that should not be duplicated between app adapters.
- Shared config orchestration built on top of `spiders-config`.
- Preview session reducer.
- Preview command application.
- Preview model projection to/from core.
- Preview snapshot/focus-tree helpers.
- Preview computation orchestration.

### Delete outright

These should not survive the extraction:

- Hardcoded master-stack snapshot override behavior.
- Layout-specific resize heuristics that bypass authored CSS/layout behavior.
- Any deprecated React-only compatibility logic once the extraction is complete.

## Data Model Direction

### Do not expose `WindowModel` directly

`WindowModel` is internal normalized state. It should stay internal.

### Reduce duplication around external window types

Current duplication exists between:

- `WindowModel`
- `WindowSnapshot`
- `PreviewSessionWindow`
- app-local preview window types

Direction:

- Promote a core-owned external DTO centered on `WindowSnapshot` or a close sibling of it.
- Let preview/session layers add only the minimal extra state they truly need.
- Stop redefining full window identity fields separately in web-specific crates.

## Config Boundary Direction

The shared runtime should be the place where config loading and runtime bootstrapping are orchestrated, but that does not mean `spiders-config` should be folded into it.

Preferred split:

- `spiders-config`: platform-neutral config schema and config service primitives.
- `spiders-wm-runtime`: shared orchestration over config + runtime + session/lifecycle behavior.
- app adapters: platform-specific discovery, storage, and environment wiring.

That keeps the layers pointed the right way:

- `spiders-config` remains foundational and reusable.
- `spiders-wm-runtime` becomes the shared runtime brain used by thin apps.
- app crates stay responsible only for environment/platform specifics.

## Lifecycle Contract

The lifecycle contract should be expressed as shared semantics in `spiders-core`, while completion timing stays platform-specific.

Shared semantic phases should cover concepts like:

- inserted
- mapped/visible
- close requested
- closing but still visually represented
- unmapped
- removed

Shared invariants should cover:

- whether a window is focusable
- whether a window is layout-eligible
- when survivor focus is selected semantically

Platform adapters then decide when a semantic phase is considered visually complete.

### Wayland

- waits on configure/commit transactions
- may keep close snapshots/overlays alive through transaction completion

### Web preview

- can complete transitions immediately or by animation timing
- may render transient closing/opening overlays
- must still obey the same semantic lifecycle contract

### Future Xorg

- will use X11 event completion rather than Wayland commit/configure barriers
- should still obey the same semantic lifecycle contract

## Migration Order

### Phase 1: Structural cleanup

1. Move `spiders-wm` under `apps/`.
2. Keep `apps/spiders-wm-playground` and `crates/spiders-web-bindings` as deprecated extraction sources only.

### Phase 2: Create `spiders-wm-runtime`

1. Create a new pure Rust crate at `crates/spiders-wm-runtime`.
2. Move typed preview/session logic out of `spiders-web-bindings`.
3. Keep the crate free of wasm-bindgen and React compatibility code.
4. Grow it toward the shared runtime/orchestration crate used by all app adapters.

### Phase 3: Move shared semantics toward core

1. Identify lifecycle/data-contract pieces duplicated in preview/runtime code.
2. Promote those into `spiders-core`.
3. Keep orchestration/runtime logic in `spiders-wm-runtime`.

### Phase 3.5: Normalize config/runtime bootstrapping

1. Move duplicated config/runtime bootstrap flow behind shared APIs in `spiders-wm-runtime`.
2. Keep `spiders-config` platform-neutral.
3. Push OS-specific config discovery out to the app adapters.

### Phase 4: Wire `apps/spiders-wm-www` directly to `spiders-wm-runtime`

1. Remove `spiders-web-bindings` from `apps/spiders-wm-www`.
2. Replace `JsValue` adapter calls with typed Rust calls.

### Phase 5: Delete deprecated hacks

1. Remove master-stack snapshot override logic.
2. Remove any preview-only behavior that bypasses authored layout/CSS rules.

### Phase 6: Deprecation cleanup

1. Stop maintaining `spiders-web-bindings`.
2. Delete it after extraction is complete.
3. Delete `apps/spiders-wm-playground` later.

## Concrete First Extraction Targets

Lowest-risk first targets:

1. Typed preview session state and command types.
2. `preview_model` and `sync_preview_state` style projection helpers.
3. Preview command reducer.
4. Layout preview computation entrypoints.

Do not preserve these when extracting:

1. `apply_snapshot_overrides`
2. hardcoded master-stack geometry rewrite logic
3. other preview heuristics that contradict authored layout behavior

## Success Criteria

After the migration:

- `apps/spiders-wm` remains a thin platform adapter.
- `apps/spiders-wm-www` becomes a thin UI shell.
- future app adapters like `apps/spiders-wm-xorg` can reuse the same shared runtime layer.
- `spiders-core` owns the shared lifecycle/focus semantics.
- `spiders-wm-runtime` owns shared runtime orchestration, including preview/session simulation.
- `spiders-config` remains platform-neutral rather than embedding OS-specific discovery assumptions.
- `spiders-web-bindings` is no longer architecturally relevant.
