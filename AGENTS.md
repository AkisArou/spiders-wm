# AGENTS

This repository is designed for agent-driven implementation.

## Mission

Build a Rust-native rewrite of `spider-wm` with:

- `smithay` for the compositor core
- `taffy` for layout computation
- `boa_engine` for JavaScript configuration and layout evaluation

The old C repo is a behavior reference only.

## Hard Decisions

These are fixed unless the user explicitly changes them:

- This is a rewrite, not a gradual migration.
- The implementation language is Rust.
- The compositor stack targets `smithay`.
- The layout engine targets `taffy`.
- The embedded JS engine target is Boa.
- User-facing config and layout authoring stays JavaScript/TypeScript-based.
- The JS environment must remain capability-limited and deterministic.

## Priority Order

1. Preserve user-facing behavior where it is intentional and documented.
2. Prefer simple, explicit Rust architecture over clever compatibility layers.
3. Keep scripting/runtime boundaries small and safe.
4. Keep specs current when implementation forces a design decision.
5. Avoid adding features that are not required by the docs.

## Preserve vs Drop

Preserve:

- tags/workspaces mental model
- declarative bindings/rules/autostart config
- structural layout tree model: `workspace`, `group`, `window`, `slot`
- separate layout CSS and effects CSS concepts
- IPC and workspace export capabilities

Drop if convenient:

- old internal file layout
- old C object naming
- QuickJS-specific or wlroots-specific implementation assumptions
- build system choices from the C repo

## Implementation Rules

- Prefer multiple focused crates over a monolith once subsystem boundaries are
  clear.
- Keep unsafe Rust isolated and justified.
- Do not expose raw Smithay state directly to JS.
- Treat JS layout functions as pure functions of context.
- Keep layout validation on the Rust side.
- Favor explicit typed intermediate representations between parsing, validation,
  resolution, and rendering.

## Expected Deliverables

When implementing a subsystem, agents should also update the relevant spec if the
implementation clarifies an open question.

If a spec and implementation conflict, either:

- change the implementation to match the spec, or
- update the spec in the same change with a clear rationale

Do not leave silent drift.

## Milestone Bias

Prefer this build order:

1. shared data model and crate structure
2. compositor skeleton with outputs, seats, surfaces, and event loop
3. config runtime and typed config model
4. layout AST validation and resolution
5. CSS-to-Taffy styling and geometry computation
6. effects styling and animations
7. IPC and workspace export
8. polish, tests, docs, and tooling
