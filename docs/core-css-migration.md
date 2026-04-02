# Core And CSS Migration Plan

## Goal

Split the current authoring/runtime/layout stack into two reusable Rust crates:

- `spiders-core`: environment-agnostic window-manager domain and tree/model logic
- `spiders-css`: reusable CSS parsing/validation/diagnostics for authored layouts

This keeps the stable tree/model concerns separate from the CSS language concerns.

## Current Status

Implemented in the repo now:

- `crates/spiders-core` exists and now owns the old `spiders-tree` surface.
- `crates/spiders-core` now also owns the extracted pure wm2 model surface in `src/wm.rs`.
- `crates/spiders-core` now also owns pure focus and directional-navigation helpers extracted from wm2.
- `crates/spiders-core` now also owns pure workspace selection and placement helpers extracted from wm2.
- `crates/spiders-css` exists and now owns the reusable style value surface plus CSS parsing/validation/compilation modules that were previously in `spiders-scene`.
- `crates/spiders-css` now exposes explicit selector/pseudo-element helpers instead of raw public Stylo implementation modules.
- `crates/spiders-core` now also owns the old `spiders-shared` API, command, snapshot, and runtime-contract surface.
- `crates/spiders-tree` has been removed from the workspace; the migration is direct and does not keep compatibility shims.
- `crates/spiders-shared` has been removed from the workspace.
- direct `spiders-core` adoption is complete in:
  - `crates/spiders-ipc`
  - `crates/spiders-config`
  - `crates/runtimes/js`
  - `crates/spiders-scene`
  - `crates/spiders-cli`
  - `crates/spiders-wm`
  - `crates/spiders-wm-river`

Validated slice:

- `cargo check -p spiders-core -p spiders-ipc -p spiders-config -p spiders-runtime-js`
- `cargo check -p spiders-core -p spiders-wm -p spiders-wm-river`
- `cargo test -p spiders-core -p spiders-runtime-js -p spiders-wm`
- `cargo test -p spiders-core -p spiders-css -p spiders-scene`
- `cargo check`

Immediate next migration targets:

1. move any remaining reusable CSS diagnostics/query helpers behind `spiders-css` instead of scene-local entrypoints
2. continue narrowing `spiders-wm2` to Smithay/runtime integration code only
3. decide whether config-side virtual modules should also move from `spiders-wm/*` to `@spiders-wm/sdk/*`

## Target Responsibilities

### `spiders-core`

Owns:

- stable IDs: `WindowId`, `OutputId`, `WorkspaceId`
- layout tree/value types: `LayoutSpace`, `LayoutRect`, `LayoutNodeMeta`
- authored layout IR: `SourceLayoutNode`, `ResolvedLayoutNode`
- match/take semantics: `MatchKey`, `MatchClause`, `WindowMatch`, `SlotTake`
- environment-agnostic WM model: windows, workspaces, outputs, seats
- pure tree/model operations
- pure focus/workspace/resize-selection logic
- tree/model validation diagnostics

Does not own:

- CSS parsing or selector/property validation
- scene/style application or geometry engine internals
- QuickJS/module graph/runtime loading
- Smithay/backend state

### `spiders-css`

Owns:

- reusable style/value types shared by parsing, scene application, and motion
- stylesheet parsing
- selector validation
- supported-property validation
- typed declaration compilation
- compiled stylesheet structures
- CSS diagnostics and error codes
- selector matching helpers reusable by scene/LSP/playground

Does not own:

- WM model state
- authored layout tree mutation or focus/workspace policy
- final scene/layout geometry calculation

### `spiders-scene`

Keeps owning:

- styled tree construction
- computed style application over resolved layout trees
- layout geometry calculation
- scene snapshots / scene requests / scene responses
- any Taffy-specific mapping and layout engine integration

After migration it depends on `spiders-core` and `spiders-css`.

## Phase 0: JS SDK Consolidation

This is adjacent work but should happen before deeper crate splits.

Status: completed for layout/runtime authoring imports. Authored layout imports now target `@spiders-wm/sdk/layout`, TSX compilation injects `@spiders-wm/sdk/jsx-runtime`, `crates/runtimes/js` reads commands/jsx-runtime from `packages/spiders-wm-sdk/src`, and the duplicate `crates/runtimes/js/sdk` tree has been removed.

1. Make `packages/spiders-wm-sdk` the only JS SDK source of truth.
2. Replace authored imports with package subpaths:
   - `spiders-wm/layout` -> `@spiders-wm/sdk/layout`
   - `spiders-wm/jsx-runtime` -> `@spiders-wm/sdk/jsx-runtime`
3. Teach `crates/runtimes/js` to embed SDK files from `packages/spiders-wm-sdk/src`.
4. Delete `crates/runtimes/js/sdk` after Rust no longer reads from it. Completed.

## Phase 1: Create `spiders-core`

### 1.1 Absorb `spiders-tree`

Move the contents of `crates/spiders-tree/src/lib.rs` into `crates/spiders-core/src/`.

Suggested module split:

- `ids.rs`
- `geometry.rs`
- `layout.rs`

Initial exports:

- `WindowId`, `OutputId`, `WorkspaceId`
- `LayoutSpace`, `LayoutRect`
- `LayoutNodeType`, `RuntimeLayoutNodeType`
- `LayoutNodeMeta`
- `MatchKey`, `MatchClause`, `WindowMatch`
- `RemainingTake`, `SlotTake`
- `SourceLayoutNode`, `ResolvedLayoutNode`

Migration rule:

- move fast and break internal crates freely when it simplifies the target architecture
- do not keep compatibility re-exports or bridge layers once direct adoption is practical

### 1.2 Absorb WM model types from `spiders-wm2`

Move these files into `crates/spiders-core/src/wm/`:

- `crates/spiders-wm2/src/model/output.rs`
- `crates/spiders-wm2/src/model/seat.rs`
- `crates/spiders-wm2/src/model/window.rs`
- `crates/spiders-wm2/src/model/wm.rs`
- `crates/spiders-wm2/src/model/workspace.rs`

Do not move:

- `crates/spiders-wm2/src/model/mod.rs` verbatim, because it currently reflects wm2-local structure

Instead create a new core surface:

- `spiders_core::wm::OutputModel`
- `spiders_core::wm::SeatModel`
- `spiders_core::wm::WindowModel`
- `spiders_core::wm::WorkspaceModel`
- `spiders_core::wm::WmModel`

### 1.3 Extract pure policy/selection logic from `spiders-wm2`

Status: completed for focus cycling, focus requests/removal, directional candidate selection, managed-window swap-position helpers, and pure workspace selection/placement helpers.

Start with functions that can operate on plain IDs and rectangles.

First candidates:

- directional focus candidate selection from `crates/spiders-wm2/src/compositor/navigation.rs`
- managed-window swap-position helpers from `crates/spiders-wm2/src/compositor/navigation.rs`
- pure workspace-selection helpers when they no longer depend on runtime side effects

Important rule:

- move only pure functions
- leave `impl SpidersWm` methods in wm2
- replace Smithay rectangles with core geometry types or simple `(x, y, width, height)` structs

### 1.4 Core adoption order

Status: completed. `spiders-shared` was used as a short-lived compatibility shim during the migration and has now been removed.

Update dependencies in this order:

1. `spiders-scene`
2. `spiders-config`
3. `spiders-runtime-js`
4. `spiders-cli`
5. `spiders-wm`
6. `spiders-wm-river`

At the end of this phase, `spiders-tree` is deleted.

## Phase 2: Create `spiders-css`

### 2.1 Move reusable CSS parsing/validation pieces from `spiders-scene`

Status: completed for the parser/compiler stack below, plus `crates/spiders-scene/src/style.rs`, which moved with them because the compiler surface depends on shared typed style values.

Move these files into `crates/spiders-css/src/`:

- `crates/spiders-scene/src/css/compile.rs`
- `crates/spiders-scene/src/css/compiled.rs`
- `crates/spiders-scene/src/css/grid.rs`
- `crates/spiders-scene/src/css/parse_values.rs`
- `crates/spiders-scene/src/css/parsing.rs`
- `crates/spiders-scene/src/css/stylo_adapter.rs`
- `crates/spiders-scene/src/css/stylo_compile.rs`
- `crates/spiders-scene/src/css/tokenizer.rs`

Keep these in `spiders-scene` for now:

- `crates/spiders-scene/src/css/taffy.rs`
- `crates/spiders-scene/src/style_calc.rs`
- `crates/spiders-scene/src/style_tree.rs`

Reason:

- `spiders-css` should stop at parsing/validation/compiled stylesheets
- scene application and Taffy mapping remain scene-specific

### 2.2 Public API of `spiders-css`

Expose:

- `CssParseError`
- `CssValueError`
- `CompiledDeclaration`
- `CompiledStyleSheet`
- keyframe-related compiled types
- selector parsing helpers needed by LSP/playground
- `parse_stylesheet(...)`
- supported-property query surface

### 2.3 Adopt `spiders-css`

Update dependencies in this order:

1. `spiders-scene`
2. `spiders-cli`
3. wasm bridge crate for browser/editor use
4. future `spiders-lsp`

The browser playground should not reimplement CSS rules in TypeScript long-term.

## Phase 3: Diagnostics Unification

Introduce stable diagnostics structs in Rust and reuse them everywhere.

### `spiders-core` diagnostics

Examples:

- invalid root node
- invalid tree structure
- invalid `take`
- unsupported match key
- duplicate IDs if enforced later

### `spiders-css` diagnostics

Examples:

- unsupported selector
- unsupported property
- invalid syntax location
- invalid CSS value

### Consumers

- CLI prints readable diagnostics
- wasm exposes JSON diagnostics to the playground
- future `spiders-lsp` maps them to LSP diagnostics

## Phase 4: Playground Integration Model

The browser should not be the source of truth for authored layout behavior.

When a new window is added in the playground, the flow should be:

1. update mock `LayoutContext.windows`
2. rerun the authored layout function from JSX
3. convert JSX to authored layout nodes via `@spiders-wm/sdk/jsx-runtime`
4. resolve claims/order using `spiders-core` logic
5. validate and compile authored CSS using `spiders-css`
6. compute styled layout / geometry using scene logic
7. render the result in the browser using the computed output

The browser's own CSS should only style the playground UI shell.

It may also style preview boxes that represent already-computed geometry, but it should not decide authored layout semantics.

### What happens when a new window arrives

Example with the current `master-stack` pattern:

1. `ctx.windows` grows from 2 to 3 windows
2. the layout function runs again
3. `showStack` flips to `true`
4. the `<slot>` claims remaining matching windows in document order
5. the authored stylesheet is compiled
6. scene/layout code computes final box geometry
7. the preview rerenders those computed boxes

That is a dataflow pipeline, not browser-native CSS layout deciding where windows go.

## Phase 5: Wasm Bridge

Add a dedicated wasm crate once the Rust boundaries are stable.

Suggested crate:

- `crates/spiders-authoring-wasm` or `crates/spiders-web-bindings`

It should expose:

- resolve layout tree
- validate tree
- parse/validate CSS
- compute preview-ready geometry when needed

Consumers:

- `apps/spiders-wm-playground`
- future `spiders-lsp` web tooling if needed

## Phase 6: Cleanup

Delete or shrink obsolete boundaries:

- remove duplicated TS playground validators once wasm exists
- remove `crates/runtimes/js/sdk`
- narrow `spiders-scene` to actual scene/layout work only
- narrow `spiders-wm2` to compositor/backend adapter work only

## Concrete First PR Sequence

1. Create `crates/spiders-core` and move the former `spiders-tree` types there.
2. Switch `spiders-scene`, `spiders-config`, `spiders-runtime-js`, `spiders-cli`, `spiders-wm`, and `crates/spiders-wm-river` to use `spiders-core` types directly, using `spiders-shared` only as a temporary compatibility shim during the cutover.
3. Delete `crates/spiders-tree`.
4. Move wm2 pure model files into `spiders-core` and make wm2 depend on them.
5. Extract pure navigation/focus helpers from wm2 into `spiders-core`.
6. Create `crates/spiders-css` from reusable `spiders-scene/src/css/*` parsing/validation pieces.
7. Make `spiders-scene` depend on `spiders-css` for stylesheet parsing.
8. Add wasm bindings over `spiders-core` + `spiders-css`.
9. Replace playground TypeScript validators/resolvers with wasm-backed calls.

## Guardrails

- Do not move Smithay-facing APIs into `spiders-core`.
- Do not move Taffy/layout-engine glue into `spiders-css`.
- Do not keep duplicate validation logic in TypeScript once Rust wasm is available.
- Prefer copy-and-switch migrations over in-place rewrites for the first extraction.
