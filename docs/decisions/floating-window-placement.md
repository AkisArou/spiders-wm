# Floating Window Placement Decision

## Status

Accepted.

## Decision

`spiders-wm` treats floating window placement as compositor-owned runtime state,
not as part of the authored tiled layout result.

The structural layout system continues to compute tiled geometry for runtime
window nodes. Floating windows then use a separate persisted placement record:

- `WindowSnapshot.floating_rect`

That rect is the canonical compositor-facing placement for a floating window.

## Rules

- tiled windows use layout-engine geometry
- floating windows use `floating_rect` when present
- floating window drag/resize updates `floating_rect`
- if a floating window crosses into another output, its `output_id` and
  `workspace_id` follow the output containing the floating window center point
- titlebar rendering and input hit-testing operate on the resolved placement,
  not by re-deriving tiled geometry ad hoc

## Why

- tiled layout and floating placement have different ownership models
- treating floating geometry as a late render override makes state drift easier
- compositor titlebars, hit-testing, and floating drag/resize need a stable
  placement model shared across runtime and rendering
- output-crossing behavior is easier to reason about when placement is explicit

## Consequences

- runtime now needs an explicit window placement layer in addition to layout
  snapshots
- render planning should consume resolved placements instead of re-checking
  `window.floating` in multiple places
- layout responses remain the source of truth for tiled windows only
- multi-output floating behavior depends on output snapshots carrying logical
  origins

## Non-Decision

- this does not define floating placement persistence across compositor restarts
- this does not yet define keyboard-driven floating move/resize behavior
- this does not yet define animation policy for transitions between tiled and
  floating modes
