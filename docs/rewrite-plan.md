# Rewrite Plan

## Goal

Deliver a Rust-native `spider-wm` that preserves the documented product shape of
the old project without inheriting its implementation constraints.

## Milestone 0: Workspace Scaffold

Deliver:

- Cargo workspace structure
- crate boundaries or placeholder crates
- shared type definitions for windows, outputs, workspaces, bindings, rules, and
  layout nodes
- top-level documentation that agents can implement against

Done when:

- the repository has an agreed subsystem map
- major open questions are recorded instead of implicit

## Milestone 1: Compositor Skeleton

Deliver:

- Smithay-based compositor startup
- output discovery and basic rendering loop
- seat/input plumbing
- xdg-shell window lifecycle integration
- internal WM state model for windows, outputs, and workspaces

Done when:

- a basic session starts
- windows can be mapped, focused, and closed
- tags/workspaces exist in Rust state even if advanced layout is not finished

## Milestone 2: Config Runtime

Deliver:

- Boa runtime bootstrapping
- config module loading
- config validation
- action bridge from JS to Rust commands
- event subscription model

Done when:

- a user config can define bindings, rules, layout defaults, and autostart
- JS receives documented events and can trigger documented actions

## Milestone 3: Layout System

Deliver:

- structural layout AST types
- AST validation
- `match` parsing
- deterministic `window` and `slot` claim resolution
- context-to-layout execution bridge through Boa

Done when:

- a layout function can return a valid tree
- the resolver can map live windows into runtime leaves deterministically

## Milestone 4: CSS Layout And Geometry

Deliver:

- structural layout CSS parser/evaluator
- selector matching for `workspace`, `group`, `window`, `#id`, `.class`
- CSS-to-Taffy style mapping
- computed geometry for tiled windows

Done when:

- `master-stack` style layouts can be expressed through CSS and layout AST nodes
- geometry is stable and recomputes correctly on window set changes

## Milestone 5: Effects And Animation

Deliver:

- effects stylesheet support
- static visual styling for real windows
- transitions, keyframes, close snapshots, and workspace transitions

Done when:

- focused/floating/fullscreen/closing states affect visuals as documented
- workspace transitions work for directional single-tag changes

## Milestone 6: IPC And External Integration

Deliver:

- local IPC server and protocol
- query/action/event surface
- `ext-workspace-v1` export
- helper CLI commands or debug tools

Done when:

- external clients can inspect state, subscribe to events, and issue commands

## Milestone 7: Stabilization

Deliver:

- test coverage for layout resolution, config validation, and effects parsing
- integration smoke tests where practical
- docs synchronized with implementation
- example config and example layouts

Done when:

- a fresh user can boot the compositor and use the documented config/layout flow

## Rules For Agents During Implementation

- Prefer vertical slices over large unfinished subsystem skeletons.
- Land typed domain models early.
- Add fixtures and tests as soon as formats or grammars stabilize.
- Keep user-facing behavior changes documented immediately.
