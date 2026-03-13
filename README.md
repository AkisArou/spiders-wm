# spiders-wm

`spiders-wm` is a clean-slate Rust rewrite of `spider-wm`.

This repository exists to make the rewrite easy for both humans and coding agents
to execute without reverse-engineering the old C codebase.

## Project Intent

- Build a keyboard-driven Wayland compositor/window manager in Rust.
- Replace `wlroots` with `smithay`.
- Replace Yoga with `taffy`.
- Replace QuickJS with Boa for config and layout evaluation.
- Preserve the best user-facing ideas from `spider-wm` while allowing internal
  architecture to change completely.

This is not an incremental migration repo. The old `spider-wm` codebase is a
reference, not a base branch.

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
- config/layout source builder: external helper, likely Rust-driven with `esbuild`
  or `swc` only where needed
- IPC: custom local IPC plus `ext-workspace-v1` export

## Repository Map

- `AGENTS.md` - execution rules and priorities for coding agents
- `docs/architecture.md` - top-level system design
- `docs/rewrite-plan.md` - milestone plan and acceptance targets
- `docs/spec/config-runtime.md` - config and JS runtime contract
- `docs/spec/layout-system.md` - layout AST, matching, CSS layout, Taffy mapping
- `docs/spec/effects-css.md` - real-window and workspace visual effects model
- `docs/spec/ipc.md` - IPC and workspace export requirements

## Intended Workspace Shape

The first implementation pass should likely split into crates roughly like:

- `spider-compositor`
- `spider-config`
- `spider-layout`
- `spider-effects`
- `spider-ipc`
- `spider-shared`
- `spider-cli`

These are planning names, not fixed API commitments.

## Reference Inputs From The Old Repo

Use `/home/akisarou/projects/spider-wm` as the historical reference for behavior,
especially:

- `/home/akisarou/projects/spider-wm/README.md`
- `/home/akisarou/projects/spider-wm/docs/layout-ast-spec.md`
- `/home/akisarou/projects/spider-wm/docs/layout-build-runtime-spec.md`
- `/home/akisarou/projects/spider-wm/docs/effects-css-spec.md`

When the old repo and this repo disagree, this repo wins.

## Working Rule

Agents should implement against the specs in this repository, not by blindly
porting C code.
