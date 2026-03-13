# SESSION_HANDOFF

This file gives the next agent enough context to continue work in this repo
without re-discovering the project intent.

## Repository

- path: `/home/akisarou/projects/spiders-wm`
- git remote: `git@github.com:AkisArou/spiders-wm.git`
- old reference repo: `/home/akisarou/projects/spider-wm`

## Project Intent

This is a fresh Rust rewrite of the private C project `spider-wm`.

Fixed technology choices:

- compositor core: `smithay`
- layout engine: `taffy`
- JS engine: `boa_engine`
- CSS parsing foundation: `cssparser`

This is not an incremental migration. The old repo is reference-only.

## Current Repo State

The repo currently contains:

- top-level rewrite docs and agent rules
- architecture and milestone planning docs
- specs for config runtime, layout system, effects CSS, IPC, and state model
- a Cargo workspace with placeholder crates

Workspace crates:

- `crates/spider-shared`
- `crates/spider-layout`
- `crates/spider-config`
- `crates/spider-effects`
- `crates/spider-ipc`
- `crates/spider-compositor`
- `crates/spider-cli`

## Important Docs

- `README.md`
- `AGENTS.md`
- `docs/architecture.md`
- `docs/rewrite-plan.md`
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
- Boa instead of QuickJS
- `cssparser` for compositor CSS parsing
- preserve JS/TS config and layout authoring model
- preserve separate structural layout CSS and effects CSS

## Immediate Recommended Next Steps

1. turn `spider-shared` placeholder types into the first real shared domain model
2. add a real `cssparser`-based parser skeleton in `spider-layout`
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
