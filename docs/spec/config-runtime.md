# Config And Runtime Spec

## Overview

The Rust rewrite keeps JavaScript or TypeScript-authored configuration and layout
definitions, but moves the runtime implementation to Rust with `boa_engine` as
the embedded JS engine.

The compositor runtime loads compiled JavaScript bundles. It does not transpile
TS or TSX internally.

## Core Design Rules

- authored source may be `config.ts` or `config.js`
- authored layouts may be `index.tsx`, `index.ts`, or `index.js`
- runtime loads compiled JavaScript bundles
- JS executes inside `boa_engine` with a limited host API
- JS does not receive raw compositor objects
- JS actions cross into Rust through explicit typed commands

## Configuration Discovery

The initial rewrite should preserve the existing discovery order unless there is a
strong implementation reason to simplify it.

Preferred authored config paths:

- `~/.config/spiders-wm/config.ts`
- `~/.config/spiders-wm/config.js`

Preferred compiled runtime path:

- `~/.local/share/spiders-wm/config.js`

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

## Layout Definitions

Named entries in `layouts` should carry enough information for Rust to select the
runtime layout source without executing discovery logic at layout time.

V1 config-facing layout definitions should include at least:

- `name`
- `module`
- `stylesheet`

Runtime-loaded source should be treated as distinct from the user-facing module
identifier/path. The config/runtime boundary may carry both:

- `module`: stable identifier or source path
- `runtime_source?`: loaded compiled JavaScript source

The preferred runtime architecture is to resolve `module` to loaded source via an
explicit loader boundary, rather than teaching every runtime consumer how to
discover or fetch source text.

`WorkspaceSnapshot.effective_layout.name` selects one of these definitions. Rust
then builds a `LayoutRequest` using the selected definition's stylesheet and the
workspace/output geometry.

The selected runtime payload should be representable as shared data with at least:

- `name`
- `module`
- `stylesheet`
- `runtime_source?`

State-driven orchestration should be able to derive the current workspace and
output from `StateSnapshot`, resolve the selected layout definition, and build a
`LayoutRequest` without compositor-specific discovery logic.

## JS Runtime Surface

The runtime should expose host modules conceptually equivalent to:

- `spiders-wm/config`
- `spiders-wm/actions`
- `spiders-wm/layout`
- `spiders-wm/api`
- `spiders-wm/jsx-runtime`

Exact implementation may change, but the intent should remain:

- config typing helpers
- declarative action helpers
- layout typing/helpers
- runtime event/query/action API
- tiny JSX runtime that creates plain layout objects only

## Safety Boundary

The JS runtime must not expose:

- `smithay` objects
- raw Wayland objects
- direct scene mutation
- arbitrary filesystem access
- network access
- timers in V1 unless explicitly documented later

## Purity Model

Layout functions should behave like pure functions.

Input:

- layout context `ctx`

V1 Rust-side contract should model this as a serializable shared payload carrying:

- full `StateSnapshot`
- selected `WorkspaceSnapshot`
- selected `OutputSnapshot?`
- selected layout payload `{ name, module, stylesheet }?`
- resolved available `space`

Output:

- a structural layout tree

The Rust config/runtime boundary should expose a trait-like evaluation surface that:

1. resolves the selected layout definition
2. builds the layout evaluation context
3. evaluates the compiled JS module into a structural layout tree

Before `boa_engine` integration lands, placeholder implementations may keep step 3
explicitly unimplemented as long as the contract is stable and tested.

The initial JS module contract should assume a single selected export named
`default`, and that export should be callable with `ctx` as its only argument.
V1 Rust runtime handling may begin with a narrow shell that evaluates module
source in `boa_engine`, invokes the callable export with the serialized layout
evaluation context, and then passes the resulting JS value into a Rust-side
conversion layer for `AuthoredLayoutNode` normalization.

Malformed returned layout objects should fail with Rust-side diagnostics that
identify layout decoding or validation errors, rather than falling back to
opaque JS-only failure modes.

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
- compiled JS bundles load in `boa_engine`
- config events and actions work through stable host APIs
- invalid config and JS runtime errors are reported clearly without crashing the
  compositor where practical
