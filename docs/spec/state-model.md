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
- event payloads and snapshots do not depend on Smithay internals leaking through
- the same state model can support both user config and external bar clients
