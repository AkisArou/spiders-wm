# Reference Repositories

## Purpose

This document records the local repositories under `/home/akisarou/projects`
that may be consulted during implementation.

These repositories are reference inputs, not source-of-truth project docs.
When they disagree with this repository's specs or decisions, `spiders-wm`
documentation wins.

## Available References

- `/home/akisarou/projects/spider-wm`
  - Historical behavior reference for the old C implementation.
  - Use for user-facing behavior, layout semantics, and feature intent.
  - Do not treat it as an implementation template.

- `/home/akisarou/projects/niri`
  - `smithay`-based compositor reference.
  - Useful for compositor structure, event loop patterns, rendering flow, and
    animation integration ideas.

- `/home/akisarou/projects/keyframe`
  - Upstream animation crate source.
  - Use for timeline design, interpolation capabilities, easing support, and API
    constraints.

- `/home/akisarou/projects/boa`
  - Upstream `boa_engine` source.
  - Useful for embedding patterns, module loading, host bindings, and runtime
    constraints.

- `/home/akisarou/projects/smithay`
  - Upstream compositor framework source and examples.
  - Use for shell handling, seat/input patterns, output management, rendering,
    and protocol integration.

- `/home/akisarou/projects/taffy`
  - Upstream layout engine source.
  - Useful for supported style behavior, geometry expectations, and layout edge
    cases.

- `/home/akisarou/projects/rust-cssparser`
  - Upstream CSS parsing reference.
  - Use for tokenizer/parser behavior and supported low-level parsing APIs.

## How To Use Them

- Start with this repository's spec docs first.
- Consult reference repos when a spec needs implementation detail, upstream API
  behavior, or a proven pattern.
- Prefer understanding over copying; adapt ideas into this project's typed Rust
  architecture.
- If a reference repo exposes a better constraint or reveals a missing spec
  decision, update this repository's docs alongside implementation.

## Best-Fit Guide

- compositor runtime questions -> `smithay`, `niri`
- animation system questions -> `keyframe`, `niri`
- JS embedding questions -> `boa_engine`
- layout engine questions -> `taffy`
- CSS parser questions -> `rust-cssparser`
- legacy behavior questions -> the old `/home/akisarou/projects/spider-wm` repo
