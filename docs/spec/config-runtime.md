# Config And Runtime Spec

## Overview

The Rust rewrite keeps JavaScript or TypeScript-authored configuration and layout
definitions, but moves the runtime implementation to Rust with Boa as the
embedded JS engine.

The compositor runtime loads compiled JavaScript bundles. It does not transpile
TS or TSX internally.

## Core Design Rules

- authored source may be `config.ts` or `config.js`
- authored layouts may be `index.tsx`, `index.ts`, or `index.js`
- runtime loads compiled JavaScript bundles
- JS executes inside Boa with a limited host API
- JS does not receive raw compositor objects
- JS actions cross into Rust through explicit typed commands

## Configuration Discovery

The initial rewrite should preserve the existing discovery order unless there is a
strong implementation reason to simplify it.

Preferred authored config paths:

- `~/.config/spider-wm/config.ts`
- `~/.config/spider-wm/config.js`

Preferred compiled runtime path:

- `~/.local/share/spider-wm/config.js`

Environment-variable overrides may exist, but they should be documented only once
the implementation is stable.

## Supported Top-Level Sections

The rewrite should preserve these top-level config concepts:

- `tags`
- `options`
- `inputs`
- `outputs`
- `layouts`
- `rules`
- `bindings`
- `autostart`
- `autostart_once`

## JS Runtime Surface

The runtime should expose host modules conceptually equivalent to:

- `spider-wm/config`
- `spider-wm/actions`
- `spider-wm/layout`
- `spider-wm/api`
- `spider-wm/jsx-runtime`

Exact implementation may change, but the intent should remain:

- config typing helpers
- declarative action helpers
- layout typing/helpers
- runtime event/query/action API
- tiny JSX runtime that creates plain layout objects only

## Safety Boundary

The JS runtime must not expose:

- Smithay objects
- raw Wayland objects
- direct scene mutation
- arbitrary filesystem access
- network access
- timers in V1 unless explicitly documented later

## Purity Model

Layout functions should behave like pure functions.

Input:

- layout context `ctx`

Output:

- a structural layout tree

The compositor owns recomputation timing, resize persistence, and all mutable WM
state.

## Event API

The rewrite should preserve these event names unless a spec change is made:

- `focus-change`
- `window-created`
- `window-destroyed`
- `window-tag-change`
- `window-floating-change`
- `window-fullscreen-change`
- `tag-change`
- `layout-change`
- `config-reloaded`

Payloads should stay data-oriented and serializable.

## Query API

The rewrite should preserve an equivalent of:

- `getState()`
- `getFocusedWindow()`
- `getCurrentMonitor()`
- `getCurrentWorkspace()`

`getState()` should expose a stable snapshot shape for JS and IPC consumers.

## WM Action API

The rewrite should preserve action concepts equivalent to:

- `spawn(command)`
- `reloadConfig()`
- `setLayout(name)`
- `cycleLayout(direction?)`
- `viewTag(tag)`
- `toggleViewTag(tag)`
- `toggleFloating()`
- `toggleFullscreen()`
- `focusDirection(direction)`
- `closeWindow()`

Lower-level binding helpers may expose additional actions, but these are the core
documented behaviors.

## Rules And Bindings

Rules and bindings remain declarative data in config.

Rules should continue to support at least:

- `app_id`
- `title`
- `tags`
- `floating`
- `fullscreen`
- `monitor`

Bindings should continue to support a `mod` alias and declarative action
descriptors.

## Builder Responsibility

The external builder is responsible for:

- resolving source entries
- bundling relative JS/TS/TSX imports
- bundling imported CSS
- emitting runtime JS bundles
- generating editor-facing support files if needed

The compositor runtime should stay independent from the exact source build tool.

## Acceptance Criteria

V1 is acceptable when:

- a user can author `config.ts`
- a user can author layouts in JS, TS, or TSX
- compiled JS bundles load in Boa
- config events and actions work through stable host APIs
- invalid config and JS runtime errors are reported clearly without crashing the
  compositor where practical
