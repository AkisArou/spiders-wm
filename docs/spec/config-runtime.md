# Config And Runtime Spec

## Overview

The Rust rewrite keeps JavaScript or TypeScript-authored configuration and layout
definitions, but moves the runtime implementation to Rust with `rquickjs` as
the embedded JS engine.

The compositor runtime loads executable JavaScript modules. TypeScript and TSX
are transpiled into cached JavaScript files so runtime execution stays in JS.

## Core Design Rules

- authored source may be `config.ts` or `config.js`
- authored layouts may be `index.tsx`, `index.ts`, or `index.js`
- runtime loads executable cached JavaScript modules
- JS executes inside `rquickjs` with a limited host API
- JS does not receive raw compositor objects
- JS actions cross into Rust through explicit typed commands

## Configuration Discovery

The initial rewrite should preserve the existing discovery order unless there is a
strong implementation reason to simplify it.

Preferred authored config paths:

- `~/.config/spiders-wm/config.ts`
- `~/.config/spiders-wm/config.js`

Preferred cached runtime entry path:

- `~/.cache/spiders-wm/config.js`

Preferred cache layout mirrors authored module structure under the prepared-config cache
root, for example:

- `~/.cache/spiders-wm/config.js`
- `~/.cache/spiders-wm/layouts/<name>/index.js`
- `~/.cache/spiders-wm/index.css`
- `~/.cache/spiders-wm/layouts/<name>/index.css`

Cache policy should stay simple in V1:

- startup/load syncs the cache when entries are missing or older than source files
- explicit build/reload may rebuild all authored apps into the cache
- no hash manifest or dependency database is required in V1

The Rust side should also expose explicit path objects for discovered authored and
runtime config locations, so startup code can pass filesystem intent around
without repeating discovery rules.

Discovery should support explicit override inputs and home-directory expansion,
so CLI and compositor startup paths can share one implementation.

Discovery may support environment-variable overrides for home, config dir, data
dir, and direct config file paths, so startup tools can override filesystem
locations without re-implementing path resolution.

Startup-facing tools should be able to emit both human-readable and structured
machine-readable status for discovery, config loading, and runtime validation.

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

- `module`: selected entry module specifier or path
- `runtime_graph?`: optional prepared in-memory JavaScript module graph

The preferred runtime architecture is to resolve `module` to loaded source via an
explicit loader boundary, rather than teaching every runtime consumer how to
discover or fetch source text.

An inline graph loader is acceptable for tests and bootstrap code, but the
runtime direction should be a filesystem-backed loader that reads cached
transpiled JavaScript modules from the resolved module path.

`WorkspaceSnapshot.effective_layout.name` selects one of these definitions. Rust
then builds a `LayoutRequest` using the selected definition's stylesheet and the
workspace/output geometry.

The selected runtime payload should be representable as shared data with at least:

- `name`
- `module`
- `stylesheet`

Loaded prepared configs should be represented separately from static config
definitions. A config-selected layout identifies what to load; a runtime-loaded
prepared layout carries the resolved cached JS source.

State-driven orchestration should be able to derive the current workspace and
output from `StateSnapshot`, resolve the selected layout definition, and build a
`LayoutRequest` without compositor-specific discovery logic.

A higher-level config runtime service should own:

- module path resolution
- prepared config loading
- loaded layout caching
- JS layout evaluation for a selected workspace

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

Before `rquickjs` integration lands, placeholder implementations may keep step 3
explicitly unimplemented as long as the contract is stable and tested.

The initial JS module contract should assume a single selected export named
`default`, and that export should be callable with `ctx` as its only argument.
V1 Rust runtime handling may begin with a narrow shell that loads a prepared
module graph in `rquickjs`, invokes the callable export with the serialized layout
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

Config modules are live runtime modules, not data-only manifests. On startup and
on config reload, the compositor should evaluate the cached `config.js` entry,
allow module side effects such as event subscription registration, read the
`default` export as declarative config data, and dispose previous subscriptions
before installing the new config runtime.

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

Decoration visibility is not a top-level config option. Window titlebar or frame
policy belongs to effects CSS on `window` selectors, with `appearance: none`
used to disable compositor-drawn decorations.

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
- resolving relative JS/TS/TSX imports into prepared runtime modules
- emitting transpiled runtime JS files into a cache directory
- collecting conventional root `index.css` stylesheets
- generating editor-facing support files if needed

The compositor runtime should stay independent from the exact source build tool.

## Acceptance Criteria

V1 is acceptable when:

- a user can author `config.ts`
- a user can author layouts in JS, TS, or TSX
- cached transpiled JS modules load in `rquickjs`
- config events and actions work through stable host APIs
- invalid config and JS runtime errors are reported clearly without crashing the
  compositor where practical
