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

These `BootstrapEvent` values are seam-input commands for bootstrap replay and
controller/application handoff. They are not a claim that every future
compositor/backend concern belongs in `spiders-runtime`; protocol-specific and
smithay-owned concerns should stay on the compositor side unless they reduce to
stable-id domain actions at this boundary.

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

`BackendDiscoveryEvent` should be read the same way: it is a translation/seam
type for backend notifications that can be expressed as stable-id topology
changes, not a request to mirror backend protocol internals into domain state.

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

When the smithay bootstrap path exports typed known-surface snapshots, the
controller-facing topology produced by draining pending discovery events should
preserve the same surface identity and role information. In particular, bootstrap
inspection should be able to assert:

- stable toplevel `window_id` to `surface_id` mapping
- typed xdg toplevel configure state snapshotting for states the bootstrap path
  actively drives (for example activated/maximized/fullscreen plus ack serial
  visibility when known)
- xdg toplevel configure inspection may also include pending configure count when
  smithay has sent toplevel configures that are not yet acked
- that includes initial xdg toplevel configures emitted from the smithay commit
  path before the client has acked its first configure
- typed xdg toplevel metadata snapshotting for fields like title, app id, and
  parent toplevel linkage when smithay reports them
- xdg toplevel metadata inspection may also include client-provided size
  constraints such as min/max size when they are available from smithay role data
- xdg toplevel metadata inspection may include client-provided window geometry
  when smithay cached xdg surface state exposes it
- typed xdg toplevel request snapshotting for compositor-handled requests like
  move, resize, minimize, and window-menu invocation when smithay reports them
- those xdg toplevel request snapshots may also expose coarse sequencing details
  such as the last request kind observed and a small request count for diagnostics/tests
- typed xdg popup configure snapshotting for reposition token, geometry, and
  reactive-positioner state when smithay drives popup configure/reposition flow
- xdg popup configure inspection may also include pending configure count when
  smithay popup configure/reposition flow has sent configures that are not yet acked
- that includes the initial xdg popup configure emitted when a popup enters the
  compositor-managed lifecycle before any reposition-specific requests occur
- xdg popup inspection may also expose coarse request sequencing details for
  popup grabs and reposition requests, such as last request kind and request count
- typed popup request snapshotting should also cover grab intent/serial when the
  xdg popup lifecycle reaches compositor-managed popup grabs
- popup `parent_surface_id` linkage, including explicit unresolved-parent state
- mapped presence while a known surface is tracked
- explicit unmap transitions when a known surface loses its mapped/buffered state
- full surface removal after smithay surface-loss events are drained
- parent-driven popup cascade behavior, where unmapping or removing a parent
  surface also unmaps or removes topology-visible child popups
- popup parent resolution is not limited to xdg toplevels; layer surfaces can
  also act as stable popup parents, and popup output attachment should follow the
  resolved layer parent when known
- when layer-shell explicitly reports a new popup for a layer parent, smithay-side
  inspection should preserve that parent linkage and output inheritance through
  the layer-shell-specific hook as well as the generic popup tracking path

The first real smithay slice should stay intentionally small:

- use a feature-gated winit-backed smithay runtime scaffold
- create the display/event-loop/backend objects
- translate one discovered seat and one discovered output into a
  `BackendTopologySnapshot`
- feed that snapshot through `SmithayAdapter` into `CompositorController`

The public entrypoint for that slice should initialize a controller from config
and snapshot state first, then hand it to the smithay-winit bootstrap helper.

That helper should also own a minimal smithay frontend state object for:

- `Display`
- compositor state
- shm state
- seat state / wl_seat creation
- initial listening socket binding

It is useful to expose this as a small `SmithayBootstrap` result that returns
both the initialized controller and the startup report, so later runtime-loop
work can build on the same bootstrap path instead of replacing it.

The next runtime-owner step should keep that shape but extend it to return a
small smithay runtime owner that keeps together:

- the `calloop::EventLoop`
- the display dispatch source
- the listening socket source and chosen socket name
- the minimal smithay frontend state object
- the winit event pump handle

That runtime owner should expose a tiny "startup cycle" boundary that pumps the
winit event source once, dispatches pending Wayland clients once, and flushes
client connections. This is enough to prove the runtime ownership boundary
without committing yet to full input routing, shell protocol setup, or
rendering-loop structure.

That startup owner now also benefits from a typed inspection boundary:

- `SmithayRuntimeSnapshot` for runtime-owned state
- `SmithayBootstrapSnapshot` for runtime state plus controller/topology summary

These snapshots should stay backend-light and test-friendly. They are useful for
asserting bootstrap/discovery flow in tests without needing full rendering or a
long-running smithay event loop.

`SmithayBootstrapSnapshot` may therefore include both the smithay-owned runtime
view and a cloned backend-agnostic topology view from the controller/session
side, so tests can compare the translated topology against the smithay known-
surface model without exposing raw smithay handles.

The initial seat setup should also include keyboard capability creation so the
bootstrap state matches the first real runtime owner more closely.

Minimal xdg-shell support is now also part of the current bootstrap slice,
including:

- xdg-shell global initialization
- initial toplevel configure handling
- typed discovery tracking for xdg toplevel and popup surfaces
- commit-time surface lifecycle tracking for xdg and unmanaged surfaces
- typed snapshot/export of known smithay surfaces, including explicit popup
  parent resolution state
- topology-level preservation of that translated xdg surface identity across the
  smithay -> controller bootstrap boundary
- explicit unmap propagation into backend-agnostic topology before final removal
- full surface removal from both smithay known-surface state and controller
  topology once xdg loss/removal events are drained

The next protocol slice now includes an initial layer-shell discovery boundary:

- layer-shell global initialization in smithay bootstrap state
- typed layer-surface discovery translated into backend-agnostic layer topology
- output attachment resolution for discovered layer surfaces using stable output ids
- minimal typed layer metadata translation, currently namespace plus requested
  layer tier
- layer metadata may also carry small policy-relevant fields from smithay such as
  keyboard interactivity and exclusive-zone intent, but only as stable typed
  values owned by this repository
- layer inspection may also carry a small configure snapshot when smithay layer
  configure/ack flow is observed, including last acked serial, pending configure
  count, and configured size
- bootstrap snapshots that can compare smithay-known layer surfaces against
  controller topology without exposing smithay layer handles
- layer-surface unmap/remap/removal transitions should preserve stable surface
  identity and output attachment until final removal

An initial clipboard/data-device inspection boundary may also exist entirely on
the smithay side when it is useful for diagnostics and tests, for example:

- data-device global initialization in smithay bootstrap state
- typed clipboard selection inspection scoped to a seat
- clipboard inspection may also include the currently focused client identity for
  the seat when smithay data-device focus is updated from seat focus changes
- stable snapshot export of selection mime types and coarse source kind
- no raw selection protocol objects crossing into backend-agnostic runtime state

The same inspection-only approach may also extend to primary selection when the
smithay bootstrap seam needs parity with clipboard focus/selection diagnostics,
for example:

- primary-selection global initialization in smithay bootstrap state
- typed primary selection inspection scoped to a seat
- primary selection inspection may also include the currently focused client
  identity for the seat when smithay primary-selection focus follows seat focus
  changes
- stable snapshot export of primary-selection mime types and coarse source kind;
  when smithay selection provider details are observable, that coarse kind may
  distinguish data-device, primary-selection, wlr-data-control, and ext-data-control
- no raw primary-selection protocol objects crossing into backend-agnostic
  runtime state

Selection-manager protocol support may also be exported as small capability
flags in smithay bootstrap/runtime inspection snapshots when that helps tests and
diagnostics assert that the expected globals were initialized. This can include:

- wl data-device support
- primary-selection support
- wlr data-control support
- ext data-control support

Those capability flags are bootstrap/runtime diagnostics only; they do not imply
that backend-agnostic runtime state owns selection protocol semantics.

Smithay bootstrap/runtime inspection may also expose a small seat/input snapshot
for the compositor-owned seat, including:

- seat name
- coarse capability presence such as keyboard, pointer, and touch
- currently focused surface id when smithay seat focus changes are observed
- coarse focused-surface summary such as focused role and resolved window id when
  focus can be tied back to a known toplevel or popup parent chain
- focused output id when the focused surface can be tied back to a smithay-known
  output attachment
- coarse cursor-image inspection such as hidden, named cursor, or cursor-surface
  mode, plus cursor surface id when the cursor image is surface-backed

This remains a smithay-side diagnostic seam only and should not replace the
backend-agnostic topology/session seat model.

Smithay bootstrap/runtime inspection may also expose a similarly small output
snapshot for diagnostics, including:

- known output ids discovered by the smithay bootstrap owner
- the currently active output id when one has been selected
- a coarse count of layer-surface output attachments tracked on the smithay side
- coarse active-output and mapped-surface counts when smithay-owned attachment
  bookkeeping can summarize how many surfaces are attached or currently mapped

This output snapshot is also diagnostics only and should stay separate from the
backend-agnostic topology output model.

When both smithay-side output summaries and backend-agnostic topology snapshots
are available during bootstrap tests, they should stay in parity for shared
coarse facts such as active output identity and mapped/attached surface counts.

The same parity rule also applies to overlapping seat facts, such as active seat
identity and focused window summaries, when both smithay diagnostics and
backend-agnostic topology snapshots export them.

Smithay-driven seat focus changes may also cross the bootstrap seam as small
discovery/bootstrap events when that is the cleanest way to keep backend-agnostic
topology focus state in sync during bootstrap/runtime tests. Those events may
carry seat name plus optional focused window/output ids only.

Smithay-owned seat removal may cross the same seam as a small typed seat-lost
event carrying only the stable seat name needed to drop backend-agnostic seat
state.

Likewise, smithay-owned output activation changes may cross the same seam as
small typed output-activation events when runtime/bootstrap tests need topology
active-output state to follow smithay state changes after initial registration.

Smithay-owned output loss may cross that seam the same way, as a small typed
output-lost event carrying only the stable output id needed to remove topology
state and clear derived attachments.

When the adapter seam is used directly for incremental lifecycle changes, the
smithay runtime/bootstrap owner should be able to hand those typed adapter
events straight to the controller without rebuilding a full topology snapshot.

That direct adapter path also covers surface lifecycle deltas such as unmap and
loss when topology tests only need stable surface ids and existing parent/output
relationships to update backend-agnostic state.

When several incremental lifecycle changes arrive together, the bootstrap owner
may batch typed adapter events and forward them in order through the same
controller-command path instead of rebuilding a larger discovery snapshot.

For initial surface discovery in adapter-driven tests, a small surface-only
discovery batch is also acceptable when it keeps the seam focused on stable
surface facts without reaching into smithay test-state mutation helpers.

The same rule applies to seat/output discovery: small typed discovery batches
are acceptable when they register stable topology facts through the adapter seam
instead of constructing those facts by mutating smithay-owned test state.

For outputs specifically, that batch should be allowed to carry a typed output
snapshot when the backend is introducing a genuinely new output that does not
already exist in the startup `StateSnapshot`.

That same typed snapshot should also be usable for incremental single-output
discovery events when the adapter is reporting one newly known output rather
than a larger discovery batch.

The real smithay bootstrap path should use that same typed output snapshot shape
when introducing the initial winit/smithay output, rather than relying on a
preexisting startup-state output id.

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
