# Dioxus Playground Port Plan

This document maps the old React playground and the migration that produced the current thin web shell.

Historical note:

- `apps/spiders-wm-playground` is now a deprecated extraction source.
- The active web shell is `apps/spiders-wm-www`.
- Shared preview/session/runtime behavior now belongs in `crates/spiders-wm-runtime`.
- Any remaining `spiders-web-bindings` references in this document refer to the legacy bridge, not the target architecture.

## Current React Playground

Main entrypoint: `apps/spiders-wm-playground/src/app.tsx`

The current app is split into three concerns:

1. Preview tab
   - `tabs/preview/index.tsx`
   - Mostly presentational.
   - Renders workspace chips, layout selector, geometry preview, unclaimed windows, diagnostics, and tree inspection.

2. Editor tab
   - `tabs/editor/index.tsx`
   - Monaco editor integration, Vim mode, synthetic SDK type libs, file tree, tab management, clipboard/download helpers.
   - This is the largest browser-specific dependency surface.

3. System tab
   - `tabs/system/index.tsx`
   - Derived state/logging view over preview state, bindings, diagnostics, and editor buffers.

Legacy bridge layer: `apps/spiders-wm-playground/src/wasm-preview.ts`

- Loads deprecated `spiders-web-bindings` wasm.
- Computes preview tree + snapshot.
- Applies preview session commands.
- Normalizes JS/wasm values back into plain objects.

## Architectural Reality

The easiest part to port is the preview and system UI.

The hardest part is not the visual preview. It is the editor and authoring pipeline:

- Monaco integration
- Monaco Vim integration
- TypeScript/TSX authoring models
- synthetic SDK type libraries
- Vite raw-module imports
- current JS layout runtime expectations

That means the correct migration order is:

1. Port preview/session state and system inspection first.
2. Port bindings-driven command dispatch second.
3. Decide editor strategy third.

## Recommended Target Architecture

### Rust-first Dioxus shell

`apps/spiders-wm-www` is the main web playground shell.

- Dioxus owns app state, tabs, preview session state, and system inspection.
- `spiders-core` remains the navigation/focus engine.
- `spiders-wm-runtime` owns the shared preview/session reducer, preview layout compute, and preview authored-layout context construction.
- Browser-only authored module graph compilation/evaluation stays in the web app adapter until a cleaner shared boundary is extracted.

### Transitional bridge for authored layouts

Keep the browser-only authored-layout bridge separate from the shared runtime layer.

Current state:

1. Current active path
   - `apps/spiders-wm-www` evaluates the authored JS module graph in-browser.
   - The app passes typed preview/session state into `spiders-wm-runtime` for shared reducer/layout behavior.
   - The app uses the shared preview layout context builder from `spiders-wm-runtime`.

2. Legacy bridge
   - `apps/spiders-wm-playground` still uses `spiders-web-bindings`.
   - This path is deprecated and should not receive new architecture work.

## Migration Phases

### Phase 1: Shell parity

Goal:

- Dioxus app has the same top-level mental model as the React app.
- Tabs: preview and system.
- Layout/scenario selection.
- Shared preview session state.

Status:

- Started in this change.

Acceptance criteria:

- Dioxus app feels like a playground shell, not a single demo page.

### Phase 2: Preview session state parity

Goal:

- Introduce a Rust-side state structure equivalent to:
  - active workspace
  - workspace names
  - windows
  - remembered focus by scope
  - shared layout adjustments keyed by stable layout node ids

Acceptance criteria:

- The Dioxus app can simulate the same focus and shared layout-adjustment transitions without reintroducing preview-only snapshot rewrite hacks.

### Phase 3: Real preview compute integration

Goal:

- Replace static demo scenarios with real preview computation outputs.
- Render snapshot geometry and resolved tree from actual preview data.

Recommended implementation:

- Use `spiders-wm-runtime` for typed preview/session/layout behavior.
- Keep browser-specific authored JS evaluation in the app adapter until a better shared runtime boundary is ready.

Acceptance criteria:

- Dioxus preview can display the same layout snapshot the React playground displays.

### Phase 4: Bindings and command dispatch

Goal:

- Parse bindings.
- Dispatch preview commands from keyboard input.
- Mirror the current `Alt+...` driven preview workflow.

Acceptance criteria:

- Keyboard-driven focus and workspace switching works in Dioxus.

### Phase 5: Editor strategy

Goal:

- Decide whether to:
  1. keep Monaco through JS interop inside Dioxus,
  2. embed a simpler Rust-native editor temporarily,
  3. or split authoring from preview and let VS Code remain the primary editor.

Recommendation:

- Do not block the Dioxus port on Monaco.
- Keep editor parity as a separate track.

### Phase 6: Full replacement decision

Goal:

- Decide if Dioxus becomes the main playground.

Decision gate:

- preview parity achieved
- bindings parity achieved
- system inspection parity achieved
- editor story is acceptable

## First Concrete Slice

Start by porting these React responsibilities:

- app shell tab model from `app.tsx`
- preview/system split from `tabs/preview` and `tabs/system`
- shared preview-oriented state in Rust

Do not try to port Monaco or TS authoring first.

## Risks

1. Monaco parity inside Dioxus may be awkward enough that the editor should remain external or hybrid.
2. Full authored-layout parity still depends on the current JS runtime assumptions.
3. The deprecated React/Vite playground may remain useful for archaeology while migration cleanups are still in progress.

## What Has Started

The Dioxus app already has:

- a Rust-first focus sandbox
- direct use of `spiders-core` directional navigation
- a tabbed playground shell with preview/system split as the first migration slice

The next implementation target should be Phase 2: a Rust-side preview session state model that mirrors the current React app more closely.
