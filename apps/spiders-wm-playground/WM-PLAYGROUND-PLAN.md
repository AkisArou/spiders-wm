# WM Playground Prototype Plan

## Goal

Prototype the layout authoring model in React before pushing deeper into compositor policy.

The playground should mirror the real authored contract:

- only supported layout JSX elements are allowed
- layout files use a custom JSX runtime
- layout functions receive the documented `LayoutContext`
- layout CSS is treated as a strict subset and validated in the playground

## Phase 1

Build the smallest useful authoring loop:

1. add a playground-local `spiders-wm/layout` contract
2. add a custom JSX runtime for layout files
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

## Constraints

- no generic DOM layouts inside `src/layouts/`
- keep layout functions deterministic
- keep the runtime small and close to `packages/spiders-wm-sdk/src/`
- prefer reuse of existing template/test layout patterns over new abstractions
