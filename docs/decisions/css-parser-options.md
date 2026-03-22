# CSS Parser Decision

## Status

Accepted (updated to match current implementation).

## Decision

`spiders-wm` currently uses a `cssparser` + Stylo-assisted pipeline for
structural layout CSS in `spiders-scene`:

- selectors are parsed/matched with the local Stylo adapter
- declaration blocks are parsed through Stylo where possible
- values are lowered into local typed declarations
- typed declarations are mapped into `taffy`

The engine keeps its own constrained runtime style model and only implements the
supported subset documented in this repository.

## Why

- the project intentionally supports a constrained structural CSS subset
- the current `cssparser` + Stylo path already integrates with selector logic
  and property/value handling used by layout runtime
- the engine still owns selector policy, lowering, validation, and `taffy` mapping locally
- it keeps parser/frontend details separate from authored CSS domain and backend mapping

## Implementation Rule

When adding CSS support:

- parse and lower through the active crate pipeline (`cssparser`/Stylo adapter
  in `spiders-scene` today)
- lower into project-specific domain/types
- keep parser/frontend AST details out of the runtime style domain
- map into local semantic values before mapping to `taffy`
- reject unsupported properties or selector forms clearly
