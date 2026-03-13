# Compositor Bootstrap Boundary

This document describes the backend-agnostic startup boundary that sits in front
of the eventual compositor skeleton/runtime loop.

## Goals

- keep backend discovery and loss handling outside core WM/layout state
- express startup and teardown through typed Rust data, not backend handles
- make bootstrap behavior testable through deterministic snapshots and event
  sequences

## Current Boundary

The current bootstrap stack in `spiders-compositor` is:

- `CompositorController` - backend-agnostic owner above host/script replay
- `CompositorHost` - minimal runtime owner above bootstrap replay
- `BootstrapRunner` - owns startup replay and diagnostics/traces
- `CompositorApp` - owns bootstrap policy and typed bootstrap event handling
- `CompositorSession` - owns WM state, runtime layout state, and topology state
- `CompositorTopologyState` - tracks outputs, seats, surfaces, and attachments

## Inputs

Initialization uses:

- discovered runtime config
- a stable `StateSnapshot`
- typed `StartupRegistration`

Ongoing bootstrap/discovery uses typed `BootstrapEvent` values for:

- seat registration/removal
- output registration/activation/enable/disable/removal
- window/popup/layer/unmanaged surface registration
- surface move/unmap/removal

## Diagnostics

`BootstrapRunner` exposes:

- `BootstrapRunTrace` for successful startup replay
- `BootstrapFailureTrace` for partial-progress failures

These traces should use stable ids and summary data only, including:

- startup registration
- applied events
- active seat/output
- workspace/window ids
- topology id lists and counts

## Scenario Helpers

In-memory startup simulations can use `BootstrapScenario` to build ordered event
sequences without going through JSON fixtures.

`BootstrapScenario` can also round-trip through the same JSON event format used by
`spiders-cli bootstrap-trace --events`, so test helpers and CLI fixtures can stay
aligned.

`BootstrapScript` now represents either a plain event-array scenario or a richer
`BootstrapTranscript`, which lets the CLI and future runtime owners share one
typed bootstrap file boundary.

`CompositorHost` can own a runner plus scenario replay at the top of this stack,
which gives the future runtime loop a small backend-agnostic owner before any
real backend integration begins.

`CompositorController` now sits one layer above that host and is responsible for
initializing from `BootstrapScript` plus replaying that script through a single
entry point. This keeps script selection and startup policy outside the lower
bootstrap runner.

The controller also owns a coarse lifecycle phase (`pending`, `bootstrapping`,
`running`, `degraded`) so outer layers like the CLI or a future backend runtime
can report bootstrap health without depending directly on runner internals.

Backend-facing discovery now has its own typed adapter boundary:

- `BackendDiscoveryEvent` models seat/output/surface discovery without exposing
  backend handles
- `BackendTopologySnapshot` models full discovered topology batches from a
  backend/source before they are expanded into incremental events
- `ControllerCommand` gives outer layers one command channel for replay scripts,
  one-off bootstrap events, backend discovery events, and backend discovery
  snapshots

This keeps the future smithay adapter responsible only for translating backend
notifications into typed controller commands.

In practice, a smithay-facing adapter should stay thin and do only three things:

1. read backend state/notifications
2. translate them into `BackendDiscoveryEvent` or `BackendTopologySnapshot`
3. submit those typed values through `CompositorController`

It should not mutate topology/session state directly.

`BackendSessionReport` now tracks the last imported backend source, generation,
and batch summary so diagnostics can explain where the current topology import
came from.

`SmithayAdapter` is the intended pre-integration seam: a thin translation module
that turns future smithay callbacks/snapshots into controller commands without
linking smithay objects into domain state.

The first real smithay slice should stay intentionally small:

- use a feature-gated winit-backed smithay runtime scaffold
- create the display/event-loop/backend objects
- translate one discovered seat and one discovered output into a
  `BackendTopologySnapshot`
- feed that snapshot through `SmithayAdapter` into `CompositorController`

That slice is only a startup/discovery proof, not full surface or rendering
integration.

JSON event scripts remain useful for CLI diagnostics and black-box integration
tests.

## Non-Goals

This layer does not own:

- raw backend object handles
- rendering state
- Wayland protocol dispatch
- smithay-specific lifetime rules

Those concerns should plug into this boundary later rather than replacing it.
