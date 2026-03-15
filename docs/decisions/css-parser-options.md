# CSS Parser Decision

## Status

Accepted.

## Decision

`spiders-wm` uses `swc_css_parser` as the stylesheet frontend for both:

- structural layout CSS parsing
- effects CSS parsing

The engine keeps its own small internal domain model and only implements the
supported CSS subset documented in this repository.

## Why

- the project intentionally supports a constrained structural CSS subset
- SWC gives us a robust stylesheet/parser frontend without forcing SWC AST into runtime logic
- the engine still owns selector policy, lowering, validation, and `taffy` mapping locally
- it keeps the parser/frontend separate from our authored CSS domain and backend mapping

## Non-Decision

- Oxc is not used for CSS parsing
- `lightningcss` is not the default parser direction
- `raffia` is not selected
- `cssparser` is no longer the chosen primary stylesheet parser

## Optional Future Helper

`parcel_selectors` may still be adopted later if selector parsing or matching
grows beyond the simple V1 subset.

## Implementation Rule

When adding CSS support:

- parse with `swc_css_parser`
- lower into project-specific domain/types
- keep SWC AST types out of the runtime style domain
- map into local semantic values before mapping to `taffy`
- reject unsupported properties or selector forms clearly
