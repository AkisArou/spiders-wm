# Layout System Spec

## Overview

The layout system turns a JS-authored structural layout tree into concrete tiled
window geometry.

The Rust rewrite preserves the old model:

- a layout function returns a tree
- the tree uses structural nodes instead of named hardcoded layouts
- Rust validates and resolves the tree
- CSS-like rules style the structural nodes
- `taffy` computes final layout geometry

## Source Node Types

The source tree supports:

- `workspace`
- `group`
- `window`
- `slot`

## Runtime Node Types

The resolved runtime tree contains:

- `workspace`
- `group`
- `window`

`slot` is source-only and expands into zero or more runtime `window` leaves.

When `slot` expands into runtime `window` leaves, each generated leaf inherits the
slot node's structural metadata (`id`, `class`, `name`, `data-*`) so structural
CSS still has a stable target after claim resolution.

## Node Semantics

### `workspace`

- root node only
- represents the monitor work area
- may contain `group`, `window`, and `slot`

### `group`

- structural container only
- layout behavior comes from CSS and `taffy`-backed style, not the tag name
- may contain `group`, `window`, and `slot`

### `window`

- singular claim node
- claims at most one matching unclaimed client
- useful for explicit placements

### `slot`

- plural claim node
- claims matching unclaimed clients
- default `take` is `remaining`

## Allowed Props

Common props:

- `id`
- `class`
- `name`
- `data-*`

`window` props:

- `match`

`slot` props:

- `match`
- `take`

## Match Grammar

Initial grammar:

```text
key="value" key="value"
```

Clauses are ANDed.

Initial supported keys:

- `app_id`
- `title`
- `class`
- `instance`
- `role`
- `shell`
- `window_type`

V1 rules:

- exact string matching only
- no regex
- no OR syntax

## `take`

Supported forms:

- omitted
- positive integer
- `remaining`

Semantics:

- omitted means `remaining`
- integer means claim up to that many matching clients
- `remaining` means claim all matching clients

## Claiming Order

Claiming is deterministic.

Rules:

1. traverse the source tree in document order
2. `window` claims the first matching unclaimed client
3. `slot` claims matching unclaimed clients up to `take`
4. later nodes only see unclaimed remaining clients

If a `window` node does not claim any live client, it still remains in the
resolved tree as an unclaimed runtime `window` leaf so authored structure can
preserve intended placement.

## Structural CSS Domain

Structural layout CSS applies after claim resolution.

V1 selector support should include at least:

- `workspace`
- `group`
- `window`
- `#id`
- `.class`

Potential future selectors such as attributes may be added later, but should not
block V1.

## CSS-To-`taffy` Mapping

The layout engine should map supported structural CSS into `taffy` style values.

V1 should focus on the subset required for existing layouts, especially:

- `display`
- `flex-direction`
- `flex-grow`
- `flex-shrink`
- `flex-basis`
- `width`
- `height`
- `min-width`
- `min-height`
- `max-width`
- `max-height`
- `gap`
- padding and margin if needed for existing layout behavior

The rewrite does not need full browser CSS support.

## Validation Rules

A layout is invalid if:

- the root is not `workspace`
- invalid node types are returned
- duplicate `id` values exist
- unsupported props are used
- `take` is invalid
- `match` cannot be parsed
- a child appears under an invalid parent

Validation happens in Rust, not in JS.

## Runtime Flow

1. JS layout module is evaluated in `boa_engine`.
2. Rust obtains the returned tree value.
3. Rust validates and normalizes the tree.
4. Rust resolves `window` and `slot` claims against live windows.
5. Rust applies structural CSS rules to the resolved runtime tree.
6. Rust creates a `taffy` tree and computes geometry.
7. Rust applies computed geometry to tiled windows.

Floating windows are a compositor-managed exception to the tiled layout result.
If a window is marked floating and has a persisted `floating_rect`, that rect
overrides the tiled layout geometry for compositor rendering and interaction
until the floating placement is changed or cleared.

Current implementation note: floating drag/resize clamps against the active
output bounds. True multi-output floating moves will require explicit output
origin metadata in shared output snapshots.

The compositor-facing boundary should use explicit shared request/response types:

- `LayoutRequest { workspace_id, output_id?, layout_name?, root, stylesheet, space }`
- `LayoutResponse { root }`

Where `space` is the available workspace size and `root` in the response is a
serializable `LayoutSnapshotNode` tree carrying final rects.

Requests should carry enough identity for tracing and policy decisions without
requiring callers to infer which workspace/output a layout result belongs to.

`LayoutSnapshotNode` should support compositor-friendly lookup by structural
node id and by claimed `window_id`, plus easy collection of runtime window nodes.

## Non-Goals For V1

- React runtime semantics
- hooks such as `useState` or `useEffect`
- imperative scene mutation from JS
- regex matching
- geometry returned directly from JS
- hardcoded named layout algorithms as the primary model

## Acceptance Criteria

V1 is acceptable when:

- a layout function can express `master-stack` style behavior
- claim resolution is deterministic
- CSS selectors correctly target resolved runtime nodes
- `taffy`-computed geometry is stable across recomputation
- invalid trees fail with clear diagnostics
