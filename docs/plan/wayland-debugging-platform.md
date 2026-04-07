# Wayland Debugging Platform

## Purpose

This document describes a long-term debugging platform for `spiders-wm`.

The goal is not to add temporary debugging hooks.

The goal is to build a finished debugging feature set for the Wayland compositor so future protocol, focus, rendering, scene, and runtime issues can be diagnosed with tooling comparable in usefulness to browser-side debugging.

## Goal

Create a durable compositor debugging platform with:

- structured event tracing
- state inspection and dumps
- protocol visibility
- scene/render inspection
- reproducible nested test environments
- optional record/replay support

This should become part of normal development infrastructure, not a one-off debug mode.

## Production Overhead Policy

This debugging platform must not impose meaningful runtime cost on normal user builds.

### Requirements

- heavy debugging capabilities must be behind compile-time features
- dormant runtime instrumentation must be near-zero overhead
- production builds must not serialize large debug payloads unless explicitly requested
- debug-only capture features must be removable from release builds entirely

### What Should Be Feature-Gated

- full state dump serialization
- scene dump export
- screenshot/debug capture commands
- protocol recording
- replay support
- verbose per-frame/per-surface logging helpers

### What May Remain In Production

- lightweight trace points
- minimal error/context logging
- cheap debug-profile plumbing that is inert unless activated

### Design Rule

If a debugging feature is expensive in CPU, memory, allocation, serialization, or binary size, it should be compiled out of normal production builds.

This policy is part of the definition of done for the debugging platform.

## Why This Is Needed

On the web we currently benefit from:

- console logs
- runtime state inspection
- network visibility
- DevTools snapshots
- screenshots and traces
- interactive debugging with Chrome MCP

On the Wayland side, we currently do not have an equivalent finished stack.

That makes compositor bugs slower to understand, especially in areas like:

- configure / ack_configure flow
- focus transitions
- seat and input handling
- surface mapping and unmapping
- frame scheduling
- relayout and scene application
- popup/subsurface issues

## Product Definition

The finished feature should provide the following capabilities.

## 1. Structured Tracing

### Requirement

All important compositor/runtime events should be emitted as structured traces.

### Coverage

- command dispatch
- host effects
- runtime events
- workspace changes
- focus changes
- map/unmap
- configure/ack_configure
- commits
- relayout scheduling
- scene application
- titlebar hit handling
- frame scheduling
- redraw/render passes
- popup lifecycle
- output sync and resize

### Output Modes

- human-readable logs
- JSON log stream
- file output for later analysis

### Why

This is the Wayland equivalent of browser console + event timeline.

## 2. On-Demand State Dumping

### Requirement

The compositor must be able to dump internal state on demand in a machine-readable format.

### Minimum dump targets

- WM model snapshot
- workspace list and active workspace
- focused surface and focused window
- mapped windows
- managed windows
- scene snapshot/root
- titlebar overlay state
- seat state
- frame-sync state
- pending configure/commit-related debug state where available

### Access Paths

- IPC command
- CLI helper
- optional periodic dump-on-error mode

### Why

This is the closest equivalent to DOM/app-state inspection in DevTools.

## 3. Protocol Visibility

### Requirement

Developers must be able to correlate compositor-side logs with client/server protocol behavior.

### Feature set

- document and integrate `WAYLAND_DEBUG` usage
- nested-launch helpers for running clients under traced conditions
- optional compositor-side protocol event logging around high-value Smithay handlers

### Why

This helps with:

- configure loops
- missing configure acknowledgements
- focus/activation issues
- popup/subsurface bugs

## 4. Scene / Render Inspection

### Requirement

We need concrete visibility into what the compositor thinks it is rendering.

### Feature set

- screenshot-on-demand
- scene snapshot dump
- titlebar overlay dump
- render frame metadata dump
- output geometry dump

### Optional future integration

- RenderDoc / GPU capture guidance
- offline frame inspection for nested runs

### Why

This is the closest analogue to browser visual inspection tools.

## 5. Nested Repro Environment

### Requirement

We need a standardized nested debugging environment for `spiders-wm`.

### Feature set

- one documented command to launch nested compositor in debug mode
- deterministic startup config
- known client set for repros
- optional scripted startup layout population

### Why

This makes compositor debugging reproducible in the same way local browser repros are reproducible.

## 6. Record / Replay

### Requirement

Longer-term, we should be able to record a debugging session and replay it.

### Scope

- record input events
- record relevant runtime events
- record relevant compositor state transitions
- optionally record high-level protocol milestones
- replay against nested compositor state

### Why

This is the strongest analogue to deterministic browser bug reproduction.

### Note

This is probably a later phase, not the first shipped slice.

## Architecture Direction

The debugging platform should not be an app-local pile of ad hoc flags.

Recommended ownership:

- `apps/spiders-wm`
  - actual host integration points
  - protocol/seat/render hooks
- `spiders-core` / `spiders-wm-runtime`
  - serializable model/runtime dump support where appropriate
- `scene` / titlebar layers
  - scene dump and visual state export helpers
- IPC layer
  - transport for debug commands

## User-Facing Surface

The finished feature should expose a coherent interface, not scattered one-off flags.

Recommended surface:

### CLI / env entry

- `spiders-wm --debug-profile <name>`
- `SPIDERS_WM_DEBUG_PROFILE=<name>`

### IPC debug commands

Examples:

- `debug.dump-state`
- `debug.dump-scene`
- `debug.dump-frame-sync`
- `debug.capture-screenshot`
- `debug.enable-trace`
- `debug.disable-trace`

### Output directory convention

- deterministic directory per session
- traces, dumps, screenshots, and optional replay data stored together

## Profiles

Use named profiles instead of many independent toggles.

Suggested profiles:

- `minimal`
  - essential tracing and state dumps
- `protocol`
  - heavier protocol-focused logging
- `render`
  - scene/render/frame inspection enabled
- `full`
  - all debugging features enabled for deep diagnosis

This makes the tool easier to use repeatedly.

## Recommended Implementation Phases

## Phase 1. Foundation

- structured tracing taxonomy
- debug profile configuration
- basic IPC debug command framework
- state dump support

Deliverable:

- usable daily debugging baseline

## Phase 2. Scene / Render Inspection

- scene dump
- screenshot capture
- frame/render metadata capture

Deliverable:

- visual compositor inspection support

## Phase 3. Protocol / Smithay Visibility

- protocol-focused tracing
- documented nested client launch helpers
- higher-value configure/commit/focus instrumentation

Current baseline:

- nested runs can already be started through `just dev`
- `spiders-wm` logs the nested `WAYLAND_DISPLAY` and `SPIDERS_WM_IPC_SOCKET`
- clients can be launched manually with `WAYLAND_DEBUG=1`
- compositor state can be captured during repro via `spiders-cli ipc-debug`
- protocol-focused logs now cover focus requests, backend focus application, initial configure sends, popup configure sends, and root commits
- render-focused logs now cover window commit, unmap, and destroy transitions

Deliverable:

- practical diagnosis for Smithay lifecycle bugs

## Phase 4. Repro Tooling

- nested debug profile launcher
- deterministic test environment helpers
- optional scripted client startup

Deliverable:

- stable repro environment

## Phase 5. Record / Replay

- event recording format
- replay runner
- deterministic replay validation

Deliverable:

- long-term deep debugging capability

## Definition Of Done

This debugging platform should be considered complete only when:

- a compositor bug can be reproduced in nested mode
- traces and dumps can be captured without editing source code
- state/scene dumps are readable and useful
- screenshots/render state can be captured on demand
- protocol/lifecycle issues can be correlated with compositor state
- the tool is documented and used as standard workflow infrastructure

## Recommendation

Do not implement piecemeal ad hoc debugging hooks anymore.

If we commit to this work, it should be treated as a productized developer platform for `spiders-wm`, with profiles, commands, dumps, and documentation from the start.
