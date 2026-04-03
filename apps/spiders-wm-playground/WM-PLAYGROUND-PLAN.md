# WM Playground Prototype Plan

## Goal

Prototype the layout authoring model in React before pushing deeper into compositor policy.

The playground should mirror the real authored contract:

- only supported layout JSX elements are allowed
- layout files use the shared SDK JSX runtime
- layout functions receive the documented `LayoutContext`
- layout CSS is treated as a strict subset and validated in the playground
- preview validation and geometry should move to Rust via a wasm bridge over `spiders-core`, `spiders-css`, and `spiders-scene`

## Phase 1

Build the smallest useful authoring loop:

1. consume the shared `@spiders-wm/sdk/layout` contract
2. consume the shared SDK JSX runtime for layout files
3. implement `Workspace`, `Group`, `Window`, and `Slot`
4. add one real layout under `src/layouts/`
5. render the resulting tree against mock window/context data
6. validate layout CSS against a strict subset

## Phase 2

Add authoring ergonomics:

1. multiple layouts and layout switching
2. editable mock window state
3. better diagnostics for invalid trees and match clauses
4. CSS subset validation that points to selector/property issues precisely

## Phase 3

Make the playground structurally closer to the Rust pipeline:

1. mirror tree validation rules more closely
2. preview claim order and unclaimed windows explicitly
3. add a geometry-oriented preview layer driven by validated layout output

## Phase 4

Historical bridge landing for the deprecated playground:

Replace the temporary TypeScript preview helpers with a Rust wasm bridge:

1. add `crates/spiders-web-bindings`
2. generate bindings directly into `apps/spiders-wm-playground/src/generated/spiders-web-bindings`
3. accept authored layout trees and mock window snapshots as JSON
4. resolve authored layout via `spiders-scene::ast`
5. compile CSS via `spiders-css`
6. compute preview geometry via `spiders-scene`
7. render the returned resolved tree and geometry in the browser

Current status:

- This bridge exists, but it is now a deprecated extraction source.
- New shared runtime/session/layout work should go into `crates/spiders-wm-runtime` instead.
- Do not expand `spiders-web-bindings` further except when harvesting/removing legacy code.

## Constraints

- no generic DOM layouts inside `src/layouts/`
- keep layout functions deterministic
- keep the runtime small and close to `packages/spiders-wm-sdk/src/`
- do not move temporary playground preview code into the SDK
- prefer reuse of existing template/test layout patterns over new abstractions
