# Focus Navigation

This document describes the current directional focus model used by the window manager and the playground preview.

## Model

Directional focus is driven by a `FocusTree` built from final window rectangles, not from authored group structure alone.

- Internally, scope identity is represented by typed `FocusScopePath` values.
- At JS and preview-session boundaries, those scope paths are serialized to strings and parsed back on the Rust side.
- The visual inference code lives in `crates/spiders-core/src/focus_visual.rs`.

## Visual Scope Inference

The visual tree is inferred from window geometry using stable ordering and projection bands.

- A scope is split horizontally or vertically by clustering windows into non-overlapping bands on each axis.
- The axis with the better split score is chosen.
- If no meaningful split exists, the scope becomes a leaf and preserves stable authored order.

Key invariants:

- A scope only has an axis when it contains at least two visual branches.
- Branch order follows visual order along the active axis.
- Leaf scopes contain only window children.

## Traversal Rules

Directional focus follows these rules:

1. Start from the focused window's scope path in the inferred visual tree.
2. Walk upward looking for the nearest ancestor whose axis matches the requested direction.
3. Prefer a real adjacent branch at that ancestor before considering wrap.
4. If moving into a scope, restore remembered focus for that scope when possible.
5. If no ancestor has a real neighbor, wrap using the outermost matching-axis scope collected during the climb.
6. If no focus tree answer exists, fall back to pure geometric nearest-neighbor selection.

This policy is what makes cases like `C -> Left -> A` and `D -> Right -> A` behave like a visual tiling layout instead of wrapping inside the nearest nested row.

## Memory

Remembered focus is tracked per scope path.

- `WmModel.last_focused_window_id_by_scope` stores the most recent focused descendant for each typed scope path.
- When a focus tree changes, remembered entries are pruned against the new tree.
- Preview state persists remembered focus as string keys, then converts them back to typed paths when rebuilding the Rust model.

## Tests

Regression coverage currently lives in:

- `crates/spiders-core/src/navigation.rs`
- `crates/spiders-core/src/focus.rs`
- `crates/spiders-web-bindings/src/lib.rs`

The navigation tests cover:

- remembered focus restoration
- ancestor-before-wrap behavior
- visual A/B/C/D traversal
- pure visual grid row wrapping
- mixed nested wrap layouts
