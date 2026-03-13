# SESSION_HANDOFF

This file gives the next agent enough context to continue work in this repo
without re-discovering the project intent.

## Repository

- path: `/home/akisarou/projects/spiders-wm`
- git remote: `git@github.com:AkisArou/spiders-wm.git`
- old reference repo path: `/home/akisarou/projects/spider-wm`

## Project Intent

This is the active Rust project for `spiders-wm`.

Fixed technology choices:

- compositor core: `smithay`
- layout engine: `taffy`
- JS engine: `boa_engine`
- animation engine: `keyframe`
- CSS parsing foundation: `cssparser`

This is not an incremental migration. The old repo is reference-only.

## Current Repo State

The repo currently contains:

- top-level rewrite docs and agent rules
- architecture and milestone planning docs
- specs for config runtime, layout system, effects CSS, IPC, and state model
- a Cargo workspace with placeholder crates

Workspace crates:

- `crates/spiders-shared`
- `crates/spiders-layout`
- `crates/spiders-config`
- `crates/spiders-effects`
- `crates/spiders-ipc`
- `crates/spiders-compositor`
- `crates/spiders-cli`

## Important Docs

- `README.md`
- `AGENTS.md`
- `docs/architecture.md`
- `docs/rewrite-plan.md`
- `docs/reference-repos.md`
- `docs/spec/config-runtime.md`
- `docs/spec/layout-system.md`
- `docs/spec/effects-css.md`
- `docs/spec/ipc.md`
- `docs/spec/state-model.md`
- `docs/decisions/css-parser-options.md`

## Decisions Already Made

- Rust-only rewrite
- `smithay` instead of `wlroots`
- `taffy` instead of Yoga
- `boa_engine` instead of QuickJS
- `keyframe` for animation timelines and interpolation
- `cssparser` for compositor CSS parsing
- preserve JS/TS config and layout authoring model
- preserve separate structural layout CSS and effects CSS

## Local Reference Repositories

The following local clones are available under `/home/akisarou/projects` for
reference during implementation:

- `/home/akisarou/projects/niri` - compositor reference, especially for
  `smithay`-oriented architecture and animation patterns
- `/home/akisarou/projects/keyframe` - animation crate source
- `/home/akisarou/projects/boa` - `boa_engine` embedding/runtime reference
- `/home/akisarou/projects/smithay` - compositor framework and example compositors
- `/home/akisarou/projects/taffy` - layout engine internals and behavior
- `/home/akisarou/projects/rust-cssparser` - CSS parser source reference

## Immediate Recommended Next Steps

1. turn `spiders-shared` placeholder types into the first real shared domain model
2. add a real `cssparser`-based parser skeleton in `spiders-layout`
3. define the first stable config/query/event payload types shared by config and IPC
4. start tests early for layout AST validation and CSS parsing

## Working Rules For Next Agents

- do not blindly port C code from `/home/akisarou/projects/spider-wm`
- implement against specs in this repo first
- if implementation clarifies a spec decision, update the spec in the same change
- keep JS runtime capability-limited
- keep unsafe Rust isolated and justified
- prefer typed intermediate models over direct dynamic plumbing

## Notes

- this repo was renamed from `spider-wm-rust` to `spiders-wm`
- some planning text may still mention the old temporary name; clean those up as
  they are encountered
