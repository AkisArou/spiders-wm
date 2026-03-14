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
- newline-delimited JSON frames for stream-friendly request and event transport
- request/response for commands and queries
- long-lived subscription stream for events

V1 may use a small JSON envelope with an optional `request_id` plus a tagged
message payload. Client messages may distinguish `query`, `action`,
`subscribe`, and `unsubscribe`, while server messages may distinguish `query`,
`event`, `action-accepted`, `subscribed`, `unsubscribed`, and `error`.

The IPC layer may also keep a small per-connection session object that:

- stores the normalized subscription topic set for that client
- forwards `query` and `action` requests upward with their `request_id`
- builds matching `query`, `action-accepted`, and `error` responses
- filters compositor events into outbound `event` messages based on the stored
  topic set

If a dedicated server helper exists, it may own multiple such sessions keyed by
connection-local client ids and:

- register and remove clients without exposing transport-specific handles
- route `query` and `action` work upward tagged with the originating client id
- return immediate subscription acknowledgements from the session layer
- broadcast compositor events only to clients whose stored subscriptions match
- provide a small request-serving helper that reads one request from a stream,
  maps it through session/server state, and writes exactly one response back to
  that same stream

The compositor crate may then mount a thin IPC host on top of that helper which:

- owns the bound Unix listener and IPC server state
- maps query requests to the current compositor `StateSnapshot`
- applies a small supported action subset through the compositor session/controller
- may keep accepted client streams and broadcast matching event messages to
  subscribed clients on those live connections
- may still leave full runtime-loop ownership and background accept/dispatch
  orchestration as later work

That compositor-mounted host now satisfies the minimum V1 live query/action path,
and can also cover basic subscribed event broadcasting, even though full
runtime-loop ownership remains follow-up work.

If a transport codec helper exists, it may treat each IPC message as one JSON
value per line, append a trailing newline on encode, and ignore surrounding
whitespace when decoding, while rejecting fully empty frames.

If a Unix socket transport helper exists, it may:

- bind a listener at a caller-provided socket path
- remove a stale socket file before binding when safe to do so
- expose small send/receive helpers for request and response envelopes
- delegate framing and parse behavior to the JSON line codec layer

Agents may implement this unless the repository later defines a stricter wire
format.

## Data Model Rules

- use stable ids where possible
- keep payloads serializable and versionable
- avoid leaking backend-specific raw handles
- make event payloads composable with JS runtime payloads where practical
- subscription messages may carry coarse topics such as `focus`, `windows`,
  `tags`, `layout`, `config`, and `all`
- topic lists should be normalized before subscription state is stored or
  compared: duplicate topics should collapse, and `all` should dominate any
  more specific topics

## Workspace Export

The compositor should expose workspace information through `ext-workspace-v1`.

The export should reflect:

- existing workspaces/tags
- current active workspace state
- monitor/output association where meaningful
- changes over time as WM state updates

The initial smithay-backed export may be read-only and map one workspace group
per enabled output, with workspace state derived from the Rust `StateSnapshot`.

## CLI Expectation

The rewrite should eventually ship a small CLI or helper commands for:

- querying state
- sending actions
- monitoring events

Before a real socket transport lands, the CLI may also ship a small in-memory
IPC smoke path that exercises request framing, session/server handling, and
response/event framing without talking to a live compositor.

Once a small Unix socket helper exists, the CLI may also offer narrow
socket-backed commands that connect to a caller-provided socket path or an
environment-provided default such as `SPIDERS_WM_IPC_SOCKET`.

The CLI may also offer a small `ipc-monitor` command that subscribes to one or
more coarse topics, waits for streamed event messages, and reports the events it
observed once the socket closes or the stream ends.

Action coverage for the CLI should include the full V1 action set, even if some
commands use compact string encodings such as `set-layout:<name>`,
`view-tag:<tag>`, `toggle-view-tag:<tag>`, or `spawn:<command>`.

This can share the same IPC protocol as third-party clients.

## Acceptance Criteria

V1 is acceptable when:

- an external client can request current state
- an external client can issue a WM command
- an external client can subscribe to event updates
- the compositor can poll/accept/dispatch pending IPC clients through a small
  nonblocking pump helper without requiring a full dedicated runtime thread
- workspace export is visible through `ext-workspace-v1`
