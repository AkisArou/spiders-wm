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

The repo now contains real implementation slices, not just planning scaffolds.

Implemented foundations include:

- shared Rust WM/layout/runtime types
- `boa_engine`-backed layout evaluation and config runtime plumbing
- validated layout resolution plus CSS-to-`taffy` geometry computation
- CLI config checking and bootstrap event fixture support
- backend-agnostic bootstrap/controller/host/session boundaries
- a split `spiders-runtime` crate for WM/topology/session domain logic
- a feature-gated first `smithay` bootstrap/runtime integration in
  `spiders-compositor`

Workspace crates:

- `crates/spiders-shared`
- `crates/spiders-layout`
- `crates/spiders-config`
- `crates/spiders-effects`
- `crates/spiders-ipc`
- `crates/spiders-runtime`
- `crates/spiders-compositor`
- `crates/spiders-cli`

## Important Docs

- `README.md`
- `AGENTS.md`
- `docs/spec/compositor-bootstrap.md`
- `docs/bootstrap-events.md`
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

## Smithay Progress So Far

The current smithay slice is intentionally still bootstrap-focused, but it is no
longer only a stub.

Implemented under the `smithay-winit` feature in `crates/spiders-compositor`:

- real winit backend startup through local smithay
- minimal Wayland `Display`, listening socket source, compositor/shm/seat state
- seat keyboard and pointer capability creation
- xdg-shell state with minimal toplevel configure handling
- runtime owner object (`SmithayWinitRuntime`) with a one-cycle startup boundary
- bootstrap wrapper (`SmithayBootstrap`) that can apply queued discovery events
  into `CompositorController`
- typed runtime/bootstrap snapshots for test inspection
- smithay-side discovered-surface tracking for toplevel/popup/unmanaged roles
- stable smithay-managed window id assignment for toplevel surfaces
- typed known-surface snapshot exports, including explicit popup parent state
- controller/topology tests covering discovery flow application

Recent commits for this slice include:

- `ea48661` - add smithay runtime owner bootstrap loop
- `79d0beb` - wire smithay winit input into runtime loop
- `28bea89` - add minimal smithay xdg shell state
- `76142b0` - track smithay xdg surfaces through bootstrap
- `39eac56` - stabilize smithay window ids across surface events
- `e23fc2a` - track smithay surface commits during bootstrap
- `e8e8027` - report smithay bootstrap runtime snapshots
- `21c6c1f` - add smithay surface role snapshot counts
- `7d500f5` - expose typed smithay known surface snapshots
- `a8f4b6e` - test smithay runtime snapshot reporting
- `3aea13f` - model smithay popup parents explicitly
- `bd5693b` - add unified smithay known surface snapshots
- `8222932` - test smithay discovery flow against controller state
- `cdad3bf` - add smithay bootstrap discovery drain helper
- `a51007c` - report smithay bootstrap topology snapshots

## Immediate Recommended Next Steps

1. extend the smithay test/bootstrap boundary from counts into richer topology assertions
2. add the next real protocol slice after xdg bootstrap, likely layer-shell or deeper xdg lifecycle handling
3. keep `spiders-runtime` backend-agnostic while growing smithay integration only in `spiders-compositor`
4. update specs whenever smithay lifecycle handling forces a boundary decision

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
