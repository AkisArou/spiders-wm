# CSS Parser Decision

## Status

Accepted.

## Decision

`spiders-wm` uses `cssparser` as the foundation for both:

- structural layout CSS parsing
- effects CSS parsing

The engine keeps its own small internal AST and only implements the supported CSS
subset documented in this repository.

## Why

- the project intentionally supports a constrained CSS subset
- `cssparser` is mature and low-level
- it fits a custom engine parser better than a browser-complete CSS stack
- it avoids pulling in a heavier general-purpose stylesheet model too early

## Non-Decision

- Oxc is not used for CSS parsing
- `lightningcss` is not the default parser direction
- `raffia` and `swc_css_parser` are not selected

## Optional Future Helper

`parcel_selectors` may still be adopted later if selector parsing or matching
grows beyond the simple V1 subset.

## Implementation Rule

When adding CSS support:

- parse with `cssparser`
- convert into project-specific AST/types
- reject unsupported properties or selector forms clearly
