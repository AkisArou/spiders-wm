# Spiders Scene Refactor

## Status

Accepted.

## Purpose

Replace the current split between `spiders-layout` and `spiders-effects` with a
single crate named `spiders-scene`.

This refactor is not a compatibility exercise. There should be no legacy
bridges, no transitional adapters, and no duplicate old/new contracts kept alive
for migration comfort. The workspace should be moved directly to the new model.

## Locked Decisions

The following are fixed for this refactor:

- the unified crate name is `spiders-scene`
- `effects.css` is removed as a term and as a user-facing file
- the user-global stylesheet is `~/.config/spiders-wm/index.css`
- each layout directory contains `index.tsx` and `index.css`
- stylesheets must be parsed once and reused, not reparsed on every layout
  request
- `spiders-wm` and other runtime crates should ask `spiders-scene` for a
  scene result, not parse or combine CSS themselves
- no compatibility layer should preserve the old string-based request model once
  the refactor lands

## Problem Statement

The current design spreads scene concerns across multiple crates and type
surfaces:

- structural layout validation and geometry live in `spiders-layout`
- visual/effects styling lives in `spiders-effects`
- config stores stylesheet contents as raw strings
- shared runtime types carry selected layout styles as strings
- layout requests pass raw stylesheet strings into the layout pipeline

This makes the runtime contract too weak for the intended compositor flow. The
runtime needs a single subsystem that can evaluate structure, geometry, and node
style together and return a scene description suitable for protocol and render
application.

## Target Architecture

### `spiders-scene`

`spiders-scene` owns:

- authored layout tree validation
- authored layout tree resolution against runtime window snapshots
- parsing and compilation of the global stylesheet
- parsing and compilation of per-layout stylesheet files
- selector matching and computed style evaluation
- mapping structural styles into `taffy`
- geometry computation
- visual and decoration style computation
- future animation planning inputs and outputs

`spiders-scene` should return a scene-oriented result that contains both:

- geometry for each node
- resolved style payloads for each node

Later this same boundary should also carry animation plans derived from style
state and `keyframe` timelines.

### `spiders-config`

`spiders-config` remains responsible for:

- config discovery
- config file loading
- authored layout/runtime preparation
- discovering the root stylesheet and layout directories
- reading stylesheet file contents from disk
- passing discovered authored assets into prepared scene artifacts

`spiders-config` should not evaluate scene geometry or compute style.

### `spiders-shared`

`spiders-shared` remains the cross-crate boundary vocabulary for:

- snapshots of windows, outputs, and workspaces
- authored layout tree data structures
- resolved layout tree data structures
- scene request and scene response data structures
- prepared runtime artifact contracts

### `spiders-wm`

`spiders-wm` should consume scene results from `spiders-scene` and translate
them into river protocol actions.

It should own protocol application for:

- placement
- border width and border color application
- CSD versus SSD policy
- titlebar/decor interaction with river protocol objects

`spiders-wm` should not parse CSS or decide style semantics on its own.

## User-Facing Filesystem Model

### Global files

The config root is `~/.config/spiders-wm/`.

Global stylesheet:

- `~/.config/spiders-wm/index.css`

Global config entry remains the existing config entrypoint unless changed in a
separate decision.

### Layout files

Each layout lives under a layout directory, for example:

- `layouts/master-stack/index.tsx`
- `layouts/master-stack/index.css`

The layout JavaScript module and layout stylesheet are a pair. They should be
discovered together by config preparation.

## No More `effects.css`

`effects.css` is removed from the design.

There is one user-global stylesheet file named `index.css`, and each layout has
its own `index.css`.

The engine may still internally separate structural style rules from visual
style rules, but that split is internal only. User-facing authoring should no
longer describe a separate effects stylesheet file.

## Parse Once, Reuse Forever Until Invalidated

Stylesheets must not be reparsed on every layout request.

The system should parse and compile stylesheets once when config artifacts are
prepared or refreshed, then reuse the compiled form across runtime requests.

### Required caching model

`spiders-scene` should maintain compiled stylesheet artifacts for:

- the global stylesheet
- each layout stylesheet

The runtime should also maintain a workspace-to-layout selection mapping so that
on each request it can quickly select:

- compiled global stylesheet
- compiled stylesheet for the active layout

Then it applies them in deterministic order:

1. global stylesheet
2. active layout stylesheet

### Invalidation rules

Compiled stylesheets are rebuilt when:

- the config is reloaded
- `index.css` at the root changes
- a layout `index.css` changes
- a layout `index.tsx` changes in a way that invalidates the prepared layout
  artifact

Runtime workspace switches should not cause stylesheet reparsing.

## Required Contract Changes

The current string-based style flow must be removed.

This means removing designs where:

- config structs store stylesheet source strings as the primary representation
- selected layout runtime structs carry stylesheet strings
- requests into the scene engine carry raw stylesheet strings

Instead, the prepared artifact contract should carry prepared scene assets.

## New Artifact Model

The prepared artifact for a selected layout should include at minimum:

- selected layout name
- prepared JS module graph for `index.tsx`
- global stylesheet source path
- layout stylesheet source path
- compiled global stylesheet
- compiled layout stylesheet

If serialized compiled artifacts prove impractical, the preparation boundary may
carry source plus a rebuild fingerprint into `spiders-scene`, but runtime scene
requests must still use cached compiled artifacts, not reparsed source strings.

The runtime-visible prepared artifact should be designed around stable scene
evaluation, not around preserving the old request fields.

## Scene Evaluation Contract

`spiders-wm` should be able to ask `spiders-scene`:

- given the selected layout artifact
- given the current workspace/output/window snapshots
- given the evaluated structural tree for the selected layout
- return the scene nodes with geometry and style

### Scene request input should include

- workspace identity
- output identity and layout space
- resolved layout tree
- runtime window/workspace/output snapshots or sufficient derived context
- reference to the cached prepared scene artifact for the selected layout

### Scene response output should include

- scene node tree
- node rects
- node style payloads
- node-to-window bindings
- future animation planning fields

The response should be rich enough for compositor application without requiring
the compositor to reinterpret CSS.

## Internal Style Domains

Even though there is no user-facing `effects.css`, the engine should still keep
an internal separation between style domains:

- structural style domain: properties that affect geometry and `taffy`
- visual style domain: properties that affect borders, decorations, titlebars,
  appearance, and future animation state

This separation is an implementation discipline, not a user-facing file split.

## River Integration Direction

`spiders-scene` should output abstract decoration and style intent.

Examples:

- desired border width
- desired border color
- desired decoration mode
- desired titlebar policy

`spiders-wm` should translate those intents into river protocol actions using
the available protocol objects, including window-management and decoration
protocol support.

The protocol layer remains compositor-specific and must not leak back into the
scene crate.

## Naming and Ownership Changes

### Crate moves

- rename `spiders-layout` to `spiders-scene`
- absorb `spiders-effects` into `spiders-scene`

### Shared type moves and redesign

`spiders-shared` should keep the canonical authored and resolved scene tree
types, but the request and response contracts should be renamed and redesigned
to match scene evaluation instead of plain layout-only evaluation.

### Config model redesign

`spiders-config` should stop treating stylesheet strings as authored config data.

The config model should point to layout directories and discovered files, while
prepared artifacts should carry scene assets that are ready for runtime use.

## Execution Plan

This plan is immediate and non-compatibility-based.

### Phase 1: Define the new shared contracts

- replace layout-only request and response types with scene-oriented request and
  response types in `spiders-shared`
- replace string stylesheet fields in shared selected-layout structures with
  prepared scene artifact references or prepared scene assets
- define scene node result structs carrying geometry plus style payloads

Exit condition:

- `spiders-shared` no longer models scene styling as raw stylesheet strings

### Phase 2: Redesign config asset discovery and preparation

- update `spiders-config` to discover `~/.config/spiders-wm/index.css`
- update layout definitions to point to layout directories rather than inline
  stylesheet strings
- require each layout directory to provide `index.tsx` and `index.css`
- prepare layout artifacts and stylesheet assets together

Exit condition:

- config preparation produces scene-ready layout artifacts instead of raw
  stylesheet strings

### Phase 3: Introduce `spiders-scene`

- create `spiders-scene`
- move structural layout validation and resolution code from `spiders-layout`
- move style and effects code from `spiders-layout` and `spiders-effects`
- define unified internal computed style structures for structural and visual
  style domains

Exit condition:

- `spiders-scene` alone owns scene evaluation logic

### Phase 4: Add compiled stylesheet caching

- add compiled stylesheet storage inside `spiders-scene`
- cache compiled global stylesheet and per-layout compiled stylesheets
- add deterministic lookup by selected layout name and workspace
- ensure cache rebuild only happens on config or file invalidation

Exit condition:

- runtime scene requests do not parse CSS source

### Phase 5: Replace runtime callers

- update `spiders-wm2` and later `spiders-wm` to ask `spiders-scene` for a
  scene result
- remove all use of old layout-only request flow
- stop passing stylesheet strings through runtime calls

Exit condition:

- compositor/runtime crates consume scene results directly

### Phase 6: Remove obsolete crates and terms

- delete `spiders-effects`
- finish renaming away `spiders-layout`
- remove `effects.css` references from docs, templates, config models, and test
  fixtures

Exit condition:

- the workspace consistently uses `spiders-scene` and `index.css`

## File-Level Refactor Targets

The following files are expected to change substantially:

- `crates/spiders-layout/**` -> moved into `crates/spiders-scene/**`
- `crates/spiders-effects/**` -> merged into `crates/spiders-scene/**`
- `crates/spiders-config/src/model.rs`
- `crates/spiders-config/src/authoring_layout.rs`
- `crates/spiders-shared/src/layout.rs`
- `crates/spiders-shared/src/runtime.rs`
- `crates/spiders-shared/src/wm.rs`
- templates and test config fixtures currently carrying `effects.css`
- docs that still refer to `effects.css`, layout-only requests, or old crate
  naming

## Guardrails

- do not preserve the old string stylesheet request API as a temporary bridge
- do not make `spiders-wm` parse CSS or own style semantics
- do not keep a separate user-facing `effects.css`
- do not reparse stylesheets on every workspace switch or layout request
- do not let compositor protocol details leak into the shared scene data model

## Expected End State

At the end of this refactor:

- the user has one root stylesheet: `~/.config/spiders-wm/index.css`
- each layout has `index.tsx` and `index.css`
- config preparation discovers and prepares scene assets
- `spiders-scene` parses stylesheets once and caches compiled forms
- runtime callers ask `spiders-scene` for scene nodes with geometry and style
- `spiders-wm` applies those results through river protocols
- the workspace no longer uses the term `effects.css`
- the workspace no longer depends on the old `spiders-layout` versus
  `spiders-effects` split

## Implementation Update (2026-03-22)

The scene internals were split into focused modules to match the architecture
direction and reduce mixed-concern files.

Completed module extractions in `crates/spiders-scene/src`:

- CSS parsing layer renamed and split:
  - `css/parsing.rs` (stylesheet parser)
  - `css/stylo_compile.rs` (Stylo declaration lowering)
  - `css/tokenizer.rs` (value tokenization)
- CSS matching extracted from `css` internals:
  - `css_matching.rs` (selector matching over compiled rules)
- Style calculator extracted from `css` internals:
  - `style_calc.rs` (computed style evaluation)
- Layout calculator extracted from `pipeline` internals:
  - `layout_calc.rs` (taffy tree build + layout collection)
- Style-tree builder extracted from `pipeline` internals:
  - `style_tree.rs` (styled node tree construction)

Pipeline boundary is now a thin facade in `pipeline/mod.rs` that orchestrates:

1. stylesheet parsing,
2. styled tree build,
3. layout computation,
4. request/response adaptation.

Validation status at extraction time:

- `cargo check -p spiders-scene` passed
- `cargo test -p spiders-scene` passed (46 tests)
- `cargo check --workspace` passed
