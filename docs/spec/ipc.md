# IPC Spec

## Overview

The Rust rewrite must provide both:

- a local IPC surface for commands, queries, and subscriptions
- `ext-workspace-v1` workspace export for protocol-aware external clients

This document defines required behavior first and leaves exact wire details as an
implementation choice unless later pinned down.

## Functional Requirements

IPC must support:

- querying compositor state
- issuing WM actions
- subscribing to state-change events
- use by bars, launchers, scripts, and debugging tools

## Required Query Concepts

IPC should expose equivalents of:

- full state snapshot
- focused window
- current monitor
- current workspace
- monitor list
- workspace/tag names

The IPC data model should align closely with the JS `query` API where practical.

## Required Action Concepts

IPC should expose equivalents of:

- reload config
- set layout
- cycle layout
- view tag
- toggle view tag
- toggle floating
- toggle fullscreen
- focus direction
- close focused window
- spawn command

## Required Event Concepts

IPC subscriptions should be able to observe equivalents of:

- focus changes
- window create/destroy
- window tag changes
- floating/fullscreen changes
- tag changes
- layout changes
- config reload

## Recommended V1 Transport

Recommended default:

- Unix domain socket
- JSON messages
- request/response for commands and queries
- long-lived subscription stream for events

Agents may implement this unless the repository later defines a stricter wire
format.

## Data Model Rules

- use stable ids where possible
- keep payloads serializable and versionable
- avoid leaking backend-specific raw handles
- make event payloads composable with JS runtime payloads where practical

## Workspace Export

The compositor should expose workspace information through `ext-workspace-v1`.

The export should reflect:

- existing workspaces/tags
- current active workspace state
- monitor/output association where meaningful
- changes over time as WM state updates

## CLI Expectation

The rewrite should eventually ship a small CLI or helper commands for:

- querying state
- sending actions
- monitoring events

This can share the same IPC protocol as third-party clients.

## Acceptance Criteria

V1 is acceptable when:

- an external client can request current state
- an external client can issue a WM command
- an external client can subscribe to event updates
- workspace export is visible through `ext-workspace-v1`
