# Titlebar History

This document is historical.

Titlebar-specific runtime, preview, config, SDK, and CSS support was removed after the implementation path proved too expensive and too coupled across subsystems.

## What Existed

The removed system previously included:

- `config.titlebars` authored JSX rules
- `@spiders-wm/sdk/titlebar`
- compositor-managed native overlays
- web preview titlebar rendering
- CSS `window::titlebar` pseudo-element support
- titlebar-specific scene snapshot/style plumbing
- dedicated titlebar crates in the workspace

## Why It Was Removed

- native behavior was deeply coupled to decoration negotiation, overlay rasterization, and input interception
- web preview and runtime snapshots had grown titlebar-specific branches instead of staying scene-first
- config, SDK, fixtures, templates, and docs had all accumulated titlebar-only surface area
- burst-open performance work showed that the native path was still dominated by repeated relayout and overlay refresh cost

## Preserved Findings

- `appearance` remains as a window decoration-policy property
- supported values are only `auto` and `none`
- scene snapshot/debug support remains, but the snapshot state is now neutral scene state rather than titlebar-shaped state
- text and style concerns discovered during the experiment should be treated as shared scene/style problems, not titlebar-local ones

## Future Reimplementation Notes

Any future attempt should start from these constraints:

1. Keep the core scene and snapshot model neutral.
2. Avoid selector, config, and SDK surface area until runtime ownership is clear.
3. Do not reintroduce `window::titlebar` or titlebar JSX as speculative API.
4. Prove decoration negotiation and performance behavior first in the native host.
5. Keep web preview as a consumer of shared scene data, not a parallel titlebar subsystem.

## Related History

- `docs/plan/titlebar-burst-open-performance.md` records the final performance investigation that happened before full removal.
