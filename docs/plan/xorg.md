# Xorg Plan

## Goal

Build `spiders-wm-x` as a first-class X11 window manager for this repository.

This document is the end-state plan.

It is not a bootstrap note, not an experiment log, and not a placeholder roadmap.

The intent is to define the final architecture we want, then execute that architecture incrementally without temporary design detours.

## Product Definition

`spiders-wm-x` should be a real X11 window manager that:

- uses `x11rb` as the X11 protocol layer
- uses `xkbcommon` for keyboard/keymap handling instead of backend-local key translation
- uses `calloop` for the X11 backend event loop once the backend needs timers, IPC, signals, and other non-X event sources
- reuses shared WM behavior from `spiders-core`, `spiders-config`, and `spiders-wm-runtime`
- manages real X11 windows across workspaces and outputs
- applies the same authored config model used elsewhere in the repo
- supports the core `spiders-wm` experience on Xorg
- has a fast, repeatable development loop built around isolated nested X servers

It should not be a compatibility shim for the Smithay compositor.

It should not be a thin demo app.

It should not be a second-class backend.

## Hard Constraints

The X11 implementation should follow these non-negotiable constraints.

### 1. No Titlebars For Now

Do not implement native custom titlebars in `spiders-wm-x` for now.

That means:

- no titlebar rendering pipeline
- no titlebar overlay state
- no titlebar hit-testing
- no titlebar config surface in the X11 host
- no dependency on titlebar behavior for window management correctness

If titlebars are revisited later, they should be a separate plan after the core X11 WM is complete and stable.

### 2. No Temporary Architecture

Every subsystem we add should fit the final backend architecture.

Avoid:

- throwaway event loops
- temporary state models that will later be replaced
- ad hoc backend-only command paths that bypass shared runtime behavior
- fake abstractions added only to “get started”

### 3. No Compatibility Layer Between Backends

Do not create a fake shared “compositor backend” abstraction just to make X11 and Smithay look structurally identical.

The correct split is:

- shared WM policy and data live in shared crates
- protocol/host execution remains backend-specific

The X11 app should be directly shaped around X11 realities.

### 4. No Duplicate WM Policy In The X11 App

Behavior such as:

- workspace selection
- focus policy
- floating/fullscreen state
- command vocabulary
- layout selection

should remain in shared crates whenever possible.

`apps/spiders-wm-x` should own X11 protocol handling and host-side execution, not fork shared WM behavior.

### 5. Development Experience Is Part Of The Product

Fast iteration is not a temporary convenience.

The final X11 backend should have a durable development workflow that makes debugging and validation fast.

## End-State Capabilities

The finished `spiders-wm-x` should provide all of the following.

## 1. X11 Window Management

- own the WM selection on the target X screen
- redirect and manage normal top-level client windows
- discover existing windows on startup
- track map/unmap/destroy/configure/property/focus lifecycle
- manage stacking, focus, geometry, and workspace assignment

## 2. Shared Config And Runtime Behavior

- discover config the same way as other backends
- load authored config through existing config/runtime infrastructure
- use shared bindings, rules, workspace names, layout selection, and commands
- support config reload without backend-specific config semantics

## 3. Workspace And Focus Model

- named workspaces
- current workspace switching
- moving windows between workspaces
- focus-next/focus-previous and directional focus where supported by shared layout state
- fullscreen and floating state managed through shared runtime commands

## 4. Layout Application

- use shared layout/runtime/scene inputs to compute geometry
- apply computed geometry to managed X11 windows
- preserve floating geometry separately from tiled layout geometry
- handle fullscreen as a backend effect with shared runtime state as the source of truth

## 5. Multi-Monitor Support

- use RandR for output discovery and change tracking
- map shared output state to real X11 outputs
- support per-output workspace attachment and focus
- respond to monitor hotplug, resize, rename, and disable events

## 6. X11 Interop Surface

- enough ICCCM support for normal client behavior
- enough EWMH support for practical desktop interoperability
- clean handling of close requests, focus, active window, workarea, and desktop metadata

This is not about supporting every historical X11 edge case.

It is about implementing the modern subset needed for a correct WM.

## 7. IPC And Diagnostics

- expose state and control through the existing IPC vocabulary where applicable
- support state dump and event inspection for development
- provide useful logging around X11 ownership, window lifecycle, and output changes

## Non-Goals

The following are out of scope for this plan.

- titlebars or decoration overlays
- compositor effects or transparency/compositing work
- systray/status bar integration
- broad backwards-compatibility work for other abandoned WM architectures
- emulating Smithay structure where X11 does not need it

## Architectural Boundary

The intended boundary in this repository should be:

- `crates/core`
  - WM model types
  - commands, queries, events, signals
  - layout/runtime domain types
- `crates/config`
  - config discovery and authored config loading
  - layout selection and rule data
- `crates/wm-runtime`
  - shared command dispatch and state transitions
- `apps/spiders-wm-x`
  - X11 connection ownership
  - X11 event loop
  - X atoms and extension negotiation
  - output/window registry
  - translation between X11 events and shared WM runtime signals
  - host-side execution of shared WM effects on X11

This is the architecture we should aim at throughout implementation.

## End-State App Structure

`apps/spiders-wm-x` should end up organized roughly around these responsibilities.

### Entry

- process startup
- logging init
- config discovery/bootstrap
- X server connection bootstrap
- backend startup mode selection

### X11 Backend Core

- connection and screen setup
- atom intern/cache
- extension negotiation
- root window ownership and event registration
- event dispatch loop built around `calloop`

### Model Registry

- managed window registry keyed by X window id
- output registry keyed by RandR/X identifiers
- mapping between X11 resources and shared `WindowId` / `OutputId`

### Runtime Bridge

- translate X11 lifecycle events into shared runtime signals
- consume shared runtime events/commands and schedule host actions
- maintain coherence between shared `WmModel` and X11 reality
- use `xkbcommon`-backed key resolution for shared bindings rather than hand-rolled keysym logic

### Host Effects

- focus window
- close window
- move/resize/configure window
- map/unmap transitions
- workspace/output visibility application
- spawn commands

### Diagnostics

- state dump
- ownership diagnostics
- event logging
- output/window inspection helpers

## X11 Protocol Scope

The final implementation should explicitly support the X11 pieces it depends on.

## Core X

- root window ownership
- event masks and event loop dispatch
- map/configure/unmap/destroy/property/client-message handling
- key grabs and pointer interaction where needed

## RandR

- enumerate outputs and CRTCs
- update output geometry on change
- react to monitor topology changes

## ICCCM

- `WM_PROTOCOLS`
- `WM_DELETE_WINDOW`
- `WM_CLASS`
- `WM_HINTS`
- `WM_NORMAL_HINTS`
- input/focus conventions needed for sane client behavior

## EWMH

- `_NET_SUPPORTED`
- `_NET_SUPPORTING_WM_CHECK`
- `_NET_ACTIVE_WINDOW`
- `_NET_CLIENT_LIST`
- `_NET_CLIENT_LIST_STACKING`
- `_NET_CURRENT_DESKTOP`
- `_NET_NUMBER_OF_DESKTOPS`
- `_NET_DESKTOP_NAMES`
- `_NET_WM_DESKTOP`
- `_NET_WM_STATE`
- `_NET_CLOSE_WINDOW`
- `_NET_WORKAREA`

Only implement EWMH items that match actual backend behavior.

Do not add fake support bits.

## Shared WM Integration Strategy

The X11 backend should drive shared state in the same direction as the other backends:

1. X11 lifecycle and output events enter the app
2. the app converts them into shared runtime signals or host interactions
3. shared runtime mutates `WmModel` and emits shared events
4. the X11 app applies resulting host-side effects back to real X windows

The shared runtime should remain the source of truth for WM policy.

The X11 backend should remain the source of truth for X11 protocol execution.

## Window Model Strategy

Each managed X11 window should have:

- stable X window id
- stable shared `WindowId`
- identity metadata from X properties
- mapping state
- workspace assignment
- output assignment
- floating/fullscreen state
- last known floating geometry where relevant

Window discovery should work in two cases:

- startup scan of existing root children
- new lifecycle events after WM ownership begins

Override-redirect windows should remain unmanaged unless there is an explicit future policy otherwise.

## Output Model Strategy

Each output should map to shared `OutputId` values and include:

- human-readable output name
- logical geometry
- enabled state
- focused workspace attachment

The output model should be updated from RandR, not hardcoded to the initial screen forever.

## Input And Bindings Strategy

The final X11 backend should support config-driven bindings through the shared binding model.

That includes:

- normalizing configured bindings through shared parsing
- installing X grabs for the needed keys/buttons
- translating X key events into shared commands
- keeping the backend-specific portion limited to grab installation and event decoding

Do not build a separate X11-only binding language.

## Layout Strategy

The X11 backend should use the same authored layout/config system as the rest of the project.

That means:

- workspace layout selection comes from shared config/runtime
- computed tiled geometry comes from shared layout/scene-related crates
- X11 host code applies geometry to client windows

Since titlebars are out of scope, the X11 layout application path should only concern client window geometry, focus, visibility, and stacking.

## Development Experience

The finished backend should support fast daily iteration without taking over the developer's real desktop session.

## Required Workflow

Primary development should happen in a nested X server such as `Xephyr`.

The expected workflow should include:

- one command to launch a nested X server for development
- one command to run `spiders-wm-x` against that nested display
- one command to run a known test client set inside that display
- one command to dump WM state during a repro

## Expected Commands

The final workflow should include a stable set of recipes similar to:

- `just x-dev`
- `just x-run`
- `just x-clients`
- `just x-dump-state`

Exact names can change, but the workflow itself should exist as part of the backend.

## Quality Requirements

- ownership failure must be explicit and easy to diagnose
- logs must clearly show which display/screen is being targeted
- state dumping must work without source edits
- monitor and window lifecycle issues must be inspectable from logs and dumps

## Execution Strategy

We still need phases, but phases should represent the order in which final subsystems land.

They should not represent temporary architecture.

## Phase 1. Final Backend Skeleton

Land the real app structure for `apps/spiders-wm-x`:

- real entrypoint
- config bootstrap
- XCB connection bootstrap
- atom cache scaffolding
- shared runtime/model ownership
- final event-loop structure
- state dump surface

Deliverable:

- the permanent backend shell exists

## Phase 2. WM Ownership And Event Dispatch

Land the final ownership and dispatch model:

- select for WM ownership on the root
- install root event masks
- define event dispatch routing by event type
- fail clearly when another WM owns the screen

Deliverable:

- the backend can run as the WM on a nested X server with the final ownership model

## Phase 3. Output And Window Discovery

Land real discovery using final data structures:

- RandR output bootstrap and updates
- startup scan of existing windows
- property loading for identity metadata
- managed/unmanaged window filtering
- initial shared state synchronization

Deliverable:

- startup produces a correct shared model of outputs and existing windows

## Phase 4. Core Window Lifecycle

Land final window management behavior:

- map/configure/unmap/destroy/property/client-message handling
- focus tracking
- close requests
- stack order updates

Deliverable:

- `spiders-wm-x` behaves like a real WM for ordinary client lifecycle

## Phase 5. Shared Command And Binding Execution

Land the runtime-controlled WM surface:

- config-driven bindings
- command execution through `spiders-wm-runtime`
- host effect execution for focus, workspace, close, floating, fullscreen, move, and resize
- config reload

Deliverable:

- the backend is driven by shared WM commands rather than bespoke X11 actions

## Phase 6. Layout And Multi-Monitor Completion

Land the final geometry/output behavior:

- full layout application for tiled windows
- floating geometry persistence
- fullscreen correctness
- multi-monitor workspace and focus behavior
- monitor hotplug/update handling

Deliverable:

- the backend supports real day-to-day X11 window management across outputs

## Phase 7. IPC And Diagnostics Completion

Land the final operability surface:

- shared IPC integration where appropriate
- durable state dump commands
- event diagnostics
- nested repro workflow and docs

Deliverable:

- the backend is practical to debug and iterate on as an everyday development target

## Definition Of Done

`spiders-wm-x` should be considered complete for this plan only when all of the following are true:

- it can own an X screen and manage ordinary client windows correctly
- startup scan and live window lifecycle stay coherent with shared WM state
- workspaces, focus, floating, fullscreen, close, and spawn work through shared runtime commands
- multi-monitor behavior works through RandR-backed output state
- config loading and reload use the shared config/runtime path
- layout selection and geometry application use the shared layout stack
- titlebars are still absent by design, not as a missing accidental gap
- nested X11 development is fast and documented
- diagnostics and state dumping are good enough for normal backend development

## Recommendation

Treat `spiders-wm-x` as a full backend product, not as a side experiment.

Implement it directly toward this architecture.

Do not spend time on temporary X11-specific shortcuts that will need to be removed later.
