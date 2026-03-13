# spiders-wm

`spiders-wm` is a clean-slate Rust rewrite of an earlier private C prototype.

This repository exists to make the rewrite easy for both humans and coding agents
to execute without reverse-engineering the old C codebase.

## Project Intent

- Build a keyboard-driven Wayland compositor/window manager in Rust.
- Replace `wlroots` with `smithay`.
- Replace Yoga with `taffy`.
- Replace QuickJS with `boa_engine` for config and layout evaluation.
- Use `keyframe` for compositor animation timelines and interpolation.
- Preserve the best user-facing ideas from the earlier prototype while allowing internal
  architecture to change completely.

This is not an incremental migration repo. The old reference codebase is not a
base branch.

## What Must Survive The Rewrite

- JavaScript or TypeScript-authored configuration.
- User-authored layout definitions that resolve into a structural layout tree.
- CSS-like layout styling for structural layout nodes.
- Separate effects styling for real window visuals and workspace transitions.
- A small, safe scripting surface that does not expose compositor internals.
- Window manager features such as tags, floating, fullscreen, monitor focus/send,
  rules, bindings, autostart, and IPC.

## What Does Not Need To Survive

- C data structures.
- Meson/Ninja build assumptions.
- QuickJS bytecode cache format.
- wlroots-specific object model.
- Yoga implementation details that are not visible in user-facing behavior.

## Target Stack

- compositor/runtime: `smithay`
- layout engine: `taffy`
- JS engine: `boa_engine`
- animation engine: `keyframe`
- config/layout source builder: external helper, likely Rust-driven with `esbuild`
  or `swc` only where needed
- IPC: custom local IPC plus `ext-workspace-v1` export

## Repository Map

- `AGENTS.md` - execution rules and priorities for coding agents
- `docs/architecture.md` - top-level system design
- `docs/rewrite-plan.md` - milestone plan and acceptance targets
- `docs/spec/compositor-bootstrap.md` - bootstrap/controller/runtime boundary and first smithay slices
- `docs/reference-repos.md` - local upstream and legacy repos worth consulting
- `docs/spec/config-runtime.md` - config and JS runtime contract
- `docs/spec/layout-system.md` - layout AST, matching, CSS layout, `taffy` mapping
- `docs/spec/effects-css.md` - real-window and workspace visual effects model
- `docs/spec/ipc.md` - IPC and workspace export requirements

## Current Workspace

The workspace currently includes:

- `spiders-compositor`
- `spiders-config`
- `spiders-layout`
- `spiders-effects`
- `spiders-ipc`
- `spiders-runtime`
- `spiders-shared`
- `spiders-cli`

`spiders-runtime` now owns the backend-agnostic WM/topology/session domain core,
while `spiders-compositor` owns the smithay-facing integration layer.

## Current Implementation Status

The repository is past the pure planning stage. Implemented slices now include:

- typed WM, topology, bootstrap, and runtime domain models in Rust
- config/runtime evaluation through `boa_engine`
- validated layout resolution and CSS-to-`taffy` geometry computation
- backend-agnostic bootstrap/controller/session boundaries
- a first feature-gated `smithay-winit` bootstrap/runtime slice with:
  - winit startup
  - minimal Wayland display/socket state
  - seat keyboard/pointer setup
  - minimal xdg-shell state
  - typed discovered-surface tracking
  - typed smithay runtime/bootstrap snapshots for tests

## Reference Inputs From The Old Repo

Use `/home/akisarou/projects/spider-wm` as the historical reference for behavior,
especially:

- `/home/akisarou/projects/spider-wm/README.md`
- `/home/akisarou/projects/spider-wm/docs/layout-ast-spec.md`
- `/home/akisarou/projects/spider-wm/docs/layout-build-runtime-spec.md`
- `/home/akisarou/projects/spider-wm/docs/effects-css-spec.md`

When the old repo and this repo disagree, this repo wins.

## Local Reference Repositories

The following local clones under `/home/akisarou/projects` may be referenced when
implementation details or upstream behavior need confirmation:

- `/home/akisarou/projects/niri` - `smithay`-based compositor reference with
  animation patterns relevant to `keyframe` usage
- `/home/akisarou/projects/keyframe` - animation crate source and API reference
- `/home/akisarou/projects/boa` - `boa_engine` source for embedding/runtime details
- `/home/akisarou/projects/smithay` - compositor framework source and examples
- `/home/akisarou/projects/taffy` - layout engine source and style behavior
- `/home/akisarou/projects/rust-cssparser` - CSS parsing reference used by this
  project's parser direction

These repositories are references only. This repository's docs and decisions
remain the source of truth for `spiders-wm`.

## Working Rule

Agents should implement against the specs in this repository, not by blindly
porting C code.
