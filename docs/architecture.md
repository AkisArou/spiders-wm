# Architecture

## Purpose

This document defines the intended top-level architecture for the Rust rewrite.
It is written as an implementation guide, not as a retrospective description of
the old C code.

## System Overview

The compositor has five main layers:

1. backend and Wayland integration via Smithay
2. window manager domain state in Rust
3. config and layout evaluation through Boa
4. layout and effects computation in Rust
5. IPC, workspace export, and helper tooling

## High-Level Data Flow

1. Rust boots Smithay, outputs, inputs, seat state, and shell integrations.
2. Rust loads compiled user config and layout bundles.
3. Boa evaluates config and layout entry modules inside a restricted runtime.
4. Rust validates config objects and layout AST results.
5. Rust resolves layout claims against live windows.
6. Rust applies layout CSS to structural nodes.
7. Rust maps computed style into Taffy nodes and computes geometry.
8. Rust applies effects styling to live window visuals and workspace snapshots.
9. Rust drives scene updates, input behavior, IPC, and protocol exports.

## Core Subsystems

## Compositor Core

Owns:

- Smithay integration
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

## Config Runtime

Owns:

- loading compiled config JS
- exposing stable host modules to Boa
- validating config shape
- dispatching config event subscribers
- executing action requests from JS through typed Rust command bridges

This layer must expose capabilities, not internal objects.

## Layout System

Owns:

- layout AST validation
- deterministic `window` and `slot` claim resolution
- CSS selector matching for structural nodes
- CSS-to-Taffy mapping
- geometry output for tiled windows

The layout system consumes pure data and produces pure computed results.

## Effects System

Owns:

- parsing and evaluating effects CSS
- static window styling
- animated transitions and keyframes
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

- `spider-shared`: shared types, ids, enums, serialization shapes
- `spider-layout`: AST, validation, CSS layout, claim resolution, Taffy mapping
- `spider-config`: config parsing, Boa integration, host modules, action bridge
- `spider-effects`: effects stylesheet model and animation state machine
- `spider-ipc`: IPC protocol and server/client helpers
- `spider-compositor`: Smithay integration and WM runtime
- `spider-cli`: helper commands such as validation, compile, inspect, IPC tools

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
- exposing Smithay or Wayland objects to JS
- arbitrary filesystem or network access from JS modules

## Main Risk Areas

- Boa module embedding and host interop ergonomics
- maintaining responsive compositor behavior while running JS safely
- matching enough CSS/Yoga behavior with Taffy-backed layout semantics
- designing IPC once instead of repeatedly refactoring it

These risks should be handled with small vertical slices, not broad speculative
framework code.
