# Architecture

## Purpose

This document defines the intended top-level architecture for the Rust rewrite.
It is written as an implementation guide, not as a retrospective description of
the old C code.

## System Overview

The compositor has five main layers:

1. backend and Wayland integration via `smithay`
2. window manager domain state in Rust
3. config and layout evaluation through `boa_engine`
4. layout and effects computation in Rust
5. IPC, workspace export, and helper tooling

## High-Level Data Flow

1. Rust boots `smithay`, outputs, inputs, seat state, and shell integrations.
2. Rust loads compiled user config and layout bundles.
3. `boa_engine` evaluates config and layout entry modules inside a restricted runtime.
4. Rust validates config objects and layout AST results.
5. Rust resolves layout claims against live windows.
6. Rust applies layout CSS to structural nodes.
7. Rust maps computed style into `taffy` nodes and computes geometry.
8. Rust applies effects styling to live window visuals and workspace snapshots.
9. Rust drives scene updates, input behavior, IPC, and protocol exports.

## Core Subsystems

## Compositor Core

Owns:

- `smithay` integration
- outputs, seats, input routing, focus, surfaces, popups, layer-shell, xdg-shell
- scene/lifecycle plumbing required for rendering and damage tracking

Must not own:

- JS evaluation policy
- layout AST parsing logic
- business logic that belongs in WM domain types

## Window Manager Domain

Owns:

- monitors/outputs
- tags/workspaces
- managed windows and their metadata
- floating/fullscreen/tiled state
- rules application
- layout selection per monitor/workspace
- persistent compositor-owned layout state

This layer is the source of truth for runtime state exposed to config and IPC.

Current implementation note:

See also: `docs/spec/compositor-bootstrap.md`

- `spiders-compositor` is converging on `CompositorSession` as the primary runtime
  orchestration boundary
- `CompositorSession` owns WM state plus compositor-owned layout/runtime state
- compositor-owned topology state for outputs, seats, and surfaces now sits beside
  WM/layout runtime state instead of leaking backend handles through the domain
- a thin compositor bootstrap/app layer can own `CompositorSession` plus startup
  seat/output registration without needing backend objects in the domain model
- startup registration policy for active seat/output selection should stay typed
  and backend-agnostic at the app/bootstrap boundary
- popup, layer, and unmanaged surface registration can also flow through that
  app/session boundary as pure topology updates before backend handles exist
- backend discovery/bootstrap should be able to feed typed seat/output/surface
  events into that boundary without exposing raw backend objects
- that event boundary should cover both registration and teardown/loss events so
  backend churn does not leak directly into domain state handling
- batch backend synchronization should also be expressible as a typed topology
  snapshot plus source/generation metadata, so initial backend imports remain
  deterministic and inspectable
- a thin bootstrap runner can own the app/session object and replay typed
  bootstrap events in order before any backend-specific runtime loop exists
- that runner can also expose typed diagnostics/traces so CLI and test tooling can
  inspect startup state without backend integration
- those diagnostics can include active seat/output plus stable workspace/window
  ids, keeping bootstrap inspection aligned with snapshot semantics
- failure traces should preserve partial progress plus the failed typed event so
  bootstrap debugging stays deterministic and backend-agnostic
- scripted bootstrap event fixtures can serve as backend-agnostic startup
  scenarios for CLI diagnostics and future skeleton testing
- session operations return a typed `SessionUpdate` containing emitted events,
  relayout status, and the current computed layout snapshot
- lower-level action helpers are internal support code, not the intended outer
  integration boundary
- a future `smithay` adapter should remain a thin translator that emits typed
  controller commands instead of mutating compositor state directly
- the first smithay integration slice should bring up a small feature-gated
  runtime scaffold for startup, seat discovery, and output discovery before any
  real surface management or rendering responsibilities expand
- that smithay scaffold now exists behind `smithay-winit` and includes a minimal
  runtime owner, xdg-shell state, startup-cycle event pumping, and typed
  smithay/runtime/bootstrap snapshots for tests and diagnostics
- current smithay-side discovery state also keeps a typed read model of known
  surfaces, including stable toplevel window ids and explicit popup parent
  resolution state, while still translating into backend-agnostic controller
  commands

## Config Runtime

Owns:

- loading compiled config JS
- exposing stable host modules to `boa_engine`
- validating config shape
- dispatching config event subscribers
- executing action requests from JS through typed Rust command bridges

This layer must expose capabilities, not internal objects.

## Layout System

Owns:

- layout AST validation
- deterministic `window` and `slot` claim resolution
- CSS selector matching for structural nodes
- CSS-to-`taffy` mapping
- geometry output for tiled windows

The layout system consumes pure data and produces pure computed results.

## Effects System

Owns:

- parsing and evaluating effects CSS
- static window styling
- animated transitions and keyframes
- `keyframe`-based animation timelines and interpolation
- closing snapshots and workspace transition snapshots

Effects styling is separate from structural layout styling on purpose.

## IPC Layer

Owns:

- local control/query protocol
- event subscription stream
- compatibility boundary for status bars and helper tools
- `ext-workspace-v1` export

## Recommended Crate Boundaries

Suggested responsibilities:

- `spiders-shared`: shared types, ids, enums, serialization shapes
- `spiders-layout`: AST, validation, CSS layout, claim resolution, `taffy` mapping
- `spiders-config`: config parsing, `boa_engine` integration, host modules, action bridge
- `spiders-effects`: effects stylesheet model and `keyframe`-backed animation state machine
- `spiders-ipc`: IPC protocol and server/client helpers
- `spiders-compositor`: `smithay` integration and WM runtime
- `spiders-cli`: helper commands such as validation, compile, inspect, IPC tools

## Source Of Truth Rules

- Rust WM state is authoritative for live compositor state.
- JS may request actions and define layouts/config, but not mutate compositor
  internals directly.
- Layout geometry is authoritative only for tiled structural placement.
- Effects styling may alter visuals, but not layout slot ownership.

## V1 Non-Goals

- porting old C APIs one-for-one
- browser-grade CSS support
- React-like runtime semantics
- exposing `smithay` or Wayland objects to JS
- arbitrary filesystem or network access from JS modules

## Main Risk Areas

- `boa_engine` module embedding and host interop ergonomics
- maintaining responsive compositor behavior while running JS safely
- matching enough CSS/Yoga behavior with `taffy`-backed layout semantics
- designing IPC once instead of repeatedly refactoring it

These risks should be handled with small vertical slices, not broad speculative
framework code.
