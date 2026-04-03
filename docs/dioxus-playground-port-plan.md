# Dioxus Playground Port Plan

This document maps the current React playground and lays out a concrete migration plan to a Dioxus-based web UI.

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

Bridge layer: `apps/spiders-wm-playground/src/wasm-preview.ts`

- Loads `spiders-web-bindings` wasm.
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

`apps/spiders-wm-web` becomes the main playground shell.

- Dioxus owns app state, tabs, preview session state, and system inspection.
- `spiders-core` remains the navigation/focus engine.
- A Rust-native preview/session model mirrors the current `PreviewSessionState` shape.

### Transitional bridge for authored layouts

In the short term, keep the authoring/runtime bridge separate from the Dioxus shell.

Two viable options:

1. Hybrid mode
   - Keep using the existing `spiders-web-bindings` preview compute path for authored layouts.
   - Dioxus calls into a narrow wasm boundary for compute/apply command operations.
   - Best short-term path.

2. Full Rust-native mode
   - Replace JS-authored layout evaluation with a Rust-native authoring/runtime layer.
   - Much larger project.
   - Not required for initial port.

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
  - master ratio by workspace
  - stack weights by workspace

Acceptance criteria:

- The Dioxus app can simulate the same focus and resize session transitions the current preview can.

### Phase 3: Real preview compute integration

Goal:

- Replace static demo scenarios with real preview computation outputs.
- Render snapshot geometry and resolved tree from actual preview data.

Recommended implementation:

- Reuse `spiders-web-bindings` behavior at a narrow boundary first.

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
3. Browser-native debugging ergonomics may still be better in the React/Vite path until preview parity is complete.

## What Has Started

The Dioxus app already has:

- a Rust-first focus sandbox
- direct use of `spiders-core` directional navigation
- a tabbed playground shell with preview/system split as the first migration slice

The next implementation target should be Phase 2: a Rust-side preview session state model that mirrors the current React app more closely.
