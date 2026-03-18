# spiders-wm2 Feature Parity Checklist

This file tracks the clean-room rebuild of `spiders-wm` behavior in `spiders-wm2`.

Guidelines:
- Preserve behavior parity where it is intentional and documented.
- Prefer clean subsystem boundaries over copying old structure.
- Keep Smithay integration as an adapter layer, not the domain core.
- Treat this file as the local source of truth for rebuild progress.

## Phase 1: Core model

- [ ] Define typed IDs for windows, workspaces, outputs, seats, and focus targets.
- [ ] Split WM state from compositor topology state.
- [ ] Add a session/runtime layer that derives view state from core state.
- [ ] Keep Smithay objects out of core model types.

## Phase 2: Compositor skeleton

- [ ] Bring up a minimal Smithay compositor loop.
- [ ] Support outputs, seats, keyboard, and pointer.
- [ ] Support xdg-shell surfaces and popups.
- [ ] Support layer-shell surfaces.
- [ ] Track surface lifecycle in topology state.
- [ ] Keep nested `winit` support behind an adapter boundary.
- [ ] Add workspace export only after basic compositor state is stable.

## Phase 3: Layout runtime

- [ ] Port the authored config -> validated layout -> resolved placement pipeline.
- [ ] Keep JS layout evaluation pure and typed.
- [ ] Validate layout structure in Rust.
- [ ] Support the structural model: `workspace`, `group`, `window`, `slot`.
- [ ] Use `taffy` for geometry computation.
- [ ] Cache and recompute layout only when config, topology, or WM state changes.

## Phase 4: Window management parity

- [ ] Workspace activation and assignment across outputs.
- [ ] Focus movement.
- [ ] Focus-after-close behavior.
- [ ] Move, swap, and close focused window.
- [ ] Send/focus monitor left-right.
- [ ] Floating window support.
- [ ] Floating geometry mutation.
- [ ] Fullscreen toggle.
- [ ] Visible-window recomputation after each action.

## Phase 5: Input and bindings

- [ ] Support declarative keybinding config.
- [ ] Dispatch bindings into WM actions.
- [ ] Support pointer focus updates.
- [ ] Support interactive move/resize for floating windows.
- [ ] Keep command spawning as a runtime side effect, not a pure reducer action.

## Phase 6: IPC

- [ ] Add a Unix socket IPC server.
- [ ] Support snapshot/state queries.
- [ ] Support current workspace query.
- [ ] Support focused window query.
- [ ] Support current output query.
- [ ] Support monitor list and workspace name queries.
- [ ] Support action submission.
- [ ] Support subscriptions/event broadcast.

## Phase 7: Effects and titlebars

- [ ] Parse effects CSS.
- [ ] Compute per-window effect state.
- [ ] Derive decoration policy from effects state.
- [ ] Add compositor-drawn titlebar planning.
- [ ] Add titlebar hit-testing and interaction wiring.

## Phase 8: Transitions and animation

- [ ] Add open transitions.
- [ ] Add close transitions.
- [ ] Add resize transitions.
- [ ] Rebuild transitions around `keyframe` timelines/interpolation.
- [ ] Add workspace transitions only when documented pseudo-states are fully supported.

## Phase 9: Missing-but-desired parity

These are documented or expected features that are not fully implemented in the current `spiders-wm` crate and should be treated as explicit `spiders-wm2` milestones.

- [ ] `autostart`
- [ ] `autostart_once`
- [ ] window rules
- [ ] input config application
- [ ] output policy application
- [ ] config runtime JS API hooks
- [ ] full workspace transition CSS semantics

## Preserve

- [ ] Keep a typed config/layout/runtime boundary.
- [ ] Keep WM state separate from topology state.
- [ ] Keep IPC snapshot/action based.
- [ ] Keep effects and titlebars as derived runtime state.
- [ ] Keep Smithay as an adapter layer.

## Do not copy literally

- [ ] Avoid carrying over bootstrap/scenario/transcript complexity unless it proves necessary.
- [ ] Avoid overgrown nested-`winit` orchestration.
- [ ] Avoid mixing backend-specific side effects into domain logic.

## Suggested milestones

- [ ] `v0`: compositor skeleton + xdg-shell/layer-shell + topology tracking.
- [ ] `v1`: workspaces, focus, floating/fullscreen, bindings, spawn, close/move/swap.
- [ ] `v2`: JS layout runtime + `taffy` geometry + IPC.
- [ ] `v3`: effects CSS + titlebars + transitions.
- [ ] `v4`: rules/autostart/inputs/outputs/config-runtime API.
