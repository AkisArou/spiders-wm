# State Model Spec

## Overview

This document defines the core runtime entities that the Rust rewrite should use
across compositor logic, config queries, layout context generation, and IPC.

The goal is to keep one shared conceptual model even if different crates expose
specialized views of it.

## Core Entities

The runtime revolves around:

- windows
- outputs
- workspaces
- tags
- seats/focus
- layout assignment

## Identity Rules

All major entities should have stable ids for the lifetime of the process.

Required id domains:

- `WindowId`
- `OutputId`
- `WorkspaceId`

Ids do not need to be user-facing, but they should be stable enough for:

- event payloads
- IPC snapshots
- JS query results
- internal maps and caches

## Window Model

Each managed window should track at least:

- stable id
- shell kind
- app id if available
- title if available
- class/instance/role if available
- mapped/unmapped state
- focused state
- floating state
- fullscreen state
- urgent state if supported
- owning output/workspace association
- current tags

The window model is the source for both rule matching and layout `match` inputs.

## Output Model

Each output should track at least:

- stable id
- connector or display name
- logical width/height
- scale
- transform
- enabled state
- active workspace/tag view

Compositor-domain note:

- backend-facing code may keep a separate topology layer for output lifecycle,
  seat focus, and surface attachment state
- that topology layer should still use stable ids and snapshot-derived values,
  not raw backend handles, as its public/domain boundary
- registration, activation, disable/enable, move, unmap, and removal events
  should all be representable in typed backend-agnostic domain inputs
- when backend adapters discover surfaces, the topology layer should preserve
  stable `surface_id`, role, optional `window_id`, optional `parent_surface_id`,
  output attachment, and mapped/removal state without leaking backend handles
- unmap and removal should stay distinct typed lifecycle outcomes: unmap keeps
  the surface identity in topology with `mapped = false`, while removal drops the
  surface entry entirely
- parent/child surface relationships should remain topology-visible enough that
  popup children can follow parent unmap/removal transitions deterministically
- popup parent linkage may target non-window surfaces such as layer surfaces;
  the backend-agnostic topology should preserve the parent `surface_id` and any
  derived output attachment without assuming the parent has a `window_id`
- protocol-specific configure/ack bookkeeping should stay compositor-owned, but
  typed snapshot state for user-visible xdg toplevel modes may cross the smithay
  inspection boundary when needed for tests and diagnostics
- similarly, smithay-owned xdg metadata like title, app id, and parent toplevel
  linkage may cross only as typed inspection data, not as protocol handles
- that typed xdg inspection metadata may also include client-provided min/max
  size constraints when they can be read from smithay role state
- the same inspection boundary may carry client-provided xdg window geometry as
  stable typed data when it is available from smithay cached state
- clipboard/data-device state should remain compositor-owned; if exported for
  diagnostics it should cross only as small typed seat-scoped inspection data
  such as selection target, mime types, and coarse source kind
- that seat-scoped inspection data may also include the currently focused client
  identity when smithay clipboard focus is derived from seat focus changes
- primary-selection state follows the same rule: it remains compositor-owned and
  may cross only as small typed seat-scoped inspection data such as selection
  target, mime types, coarse source kind, and focused client identity
- when smithay makes the originating selection provider observable, that coarse
  source kind may distinguish data-device, primary-selection, wlr-data-control,
  and ext-data-control, but still only as diagnostics-facing typed data
- bootstrap/runtime inspection may also expose coarse selection-protocol
  capability flags so tests can assert which smithay selection globals are
  active, but those flags remain diagnostics only and do not move protocol
  ownership into backend-agnostic state
- a similarly small smithay-side seat/input inspection snapshot may expose the
  compositor-owned seat name, coarse capability presence, focused surface id,
  focused role/window/output summary, and coarse cursor-image state for
  diagnostics, while backend-agnostic seat ownership still remains in the
  topology/session state model
- smithay-side diagnostics may also expose a small output inspection snapshot
  with known output ids, active output id, coarse smithay-owned attachment
  counts, and mapped-surface summary counts, while backend-agnostic output
  ownership still remains in topology/session state
- where backend-agnostic topology exports the same coarse facts, tests should
  assert parity between smithay-side output diagnostics and topology output
  state instead of letting those summaries drift independently
- the same applies to overlapping seat diagnostics such as active seat identity
  and focused window/output summaries when both sides export them
- when smithay focus changes need to update backend-agnostic topology during
  bootstrap/runtime, that handoff should happen through small typed seat-focus
  events carrying seat name plus optional focused window/output ids rather than
  leaking smithay seat or surface objects across the boundary
- the same pattern applies to smithay-owned output activation changes: hand off
  only the typed output id needed to update backend-agnostic topology state
- layer-shell configure bookkeeping also remains compositor-owned, but a small
  typed inspection snapshot for last acked serial, pending configure count, and
  configured size may cross the smithay seam for diagnostics and tests
- layer-parented popup linkage also remains compositor-owned, but smithay-side
  diagnostics may record that linkage explicitly when it arrives through the
  layer-shell popup hook so tests can assert parent/output inheritance parity
- popup reposition/configure bookkeeping should also stay compositor-owned, but
  typed inspection snapshots for geometry, reposition token, pending configure
  count, and reactive popup state may cross the smithay boundary for tests and
  diagnostics
- those popup pending counts should include the initial configure send that
  occurs before any later reposition-driven popup updates
- popup grab/reposition bookkeeping may also expose small sequencing diagnostics
  such as last request kind and request count, while popup request handling
  itself remains compositor-owned
- xdg toplevel configure bookkeeping follows the same rule: it remains
  compositor-owned, but typed inspection snapshots may expose activated,
  fullscreen, maximized, acked serial, and pending configure count for tests and
  diagnostics
- those pending counts should include initial configure sends emitted before the
  client has acked its first xdg toplevel configure
- xdg toplevel request bookkeeping may also expose small sequencing diagnostics
  such as last request kind and request count, while request handling itself
  remains compositor-owned
- likewise, compositor-handled xdg toplevel requests such as move, resize,
  minimize, and window-menu invocation may cross only as typed inspection data
- popup grab requests follow the same rule: smithay owns the protocol objects and
  grab mechanics, while typed grab intent/serial may cross only as inspection data
- layer surfaces should also preserve stable surface identity plus resolved
  output attachment at the topology boundary, even though their protocol/runtime
  behavior remains backend-specific
- for layer surfaces, unmap/remap should preserve the resolved output attachment
  across lifecycle transitions until the surface is finally removed
- backend-agnostic layer surface state may also carry small typed metadata such
  as namespace and requested layer tier, as long as raw backend protocol objects
  do not cross the boundary
- that metadata may include policy-relevant fields like keyboard interactivity
  and exclusive-zone intent when they are represented as repository-owned enums

## Workspace Model

The old project mixes tag and workspace concepts in user-facing language. The
rewrite should keep this mental model but define it explicitly.

Recommended model:

- tags are named user-visible grouping units
- each output has a current tag view
- a workspace snapshot is the output-local currently visible state derived from
  tag selection and layout choice

If implementation later distinguishes tags and workspaces more strongly, the docs
should be updated consistently across IPC and JS APIs.

## Layout Assignment Model

The runtime should support:

- global default layout
- per-tag layout override
- per-output layout override
- current effective layout per output/workspace view

Effective layout lookup should preserve the documented order from the old project
unless intentionally changed in spec.

## Query Snapshot Model

The data returned by JS `query` APIs and IPC should come from a stable snapshot
layer rather than raw mutable backend state.

That snapshot should include:

- focused window
- current output
- current workspace
- outputs
- visible windows
- tag names

Implementation note:

- compositor bootstrap/session code may own additional topology state for seats,
  outputs, and surfaces, but query-facing data should still derive from stable
  snapshot semantics rather than backend object identity

## Event Model

Events should reference stable ids and include enough denormalized data to be
useful without requiring immediate follow-up queries in common cases.

Recommended rule:

- include ids always
- include commonly needed names/flags where cheap
- avoid raw backend handles or opaque internal pointers

## Acceptance Criteria

V1 is acceptable when:

- config runtime, layout system, and IPC can all agree on shared entity meanings
- event payloads and snapshots do not depend on `smithay` internals leaking through
- the same state model can support both user config and external bar clients
