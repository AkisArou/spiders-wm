# IPC Unification Plan

## Goal

Unify IPC across:

- `apps/spiders-wm`
- `apps/spiders-wm-x`
- `apps/spiders-wm-www`

so they share:

- one protocol vocabulary
- one request-routing model
- one subscription/event model
- one minimal backend integration trait

The native backends should share one `calloop`-based IPC implementation.

The browser should use the same logical IPC surface through a browser-specific crate, without any UI in the crate itself.

## Hard Decisions

These are design constraints for the refactor.

## 1. Native IPC Uses `calloop`

Both native window managers should use `calloop` for IPC.

That means:

- keep Wayland on `calloop`
- move X11 IPC to `calloop`
- remove the temporary X11 `on_idle` IPC path

There is no advantage in preserving the X11 polling path.

It is temporary, more error-prone, and weaker than readiness-driven handling.

## 2. Crate Split Is `ipc/{core,native,browser}`

The IPC implementation should be split into:

- `crates/ipc/core`
- `crates/ipc/native`
- `crates/ipc/browser`

## 3. Browser Crate Is UI-Free

`crates/ipc/browser` should not contain UI.

It should expose browser-appropriate APIs built with `web-sys` and `js-sys`.

Any fake terminal or debugging panel belongs in `apps/spiders-wm-www`.

## 4. WM Implementations Stay Minimal

The WM apps should not own shared IPC transport/session policy.

They should only:

- implement a small handler trait
- provide query / command / debug behavior
- push runtime events into the shared broadcaster

## Why

Today the repository already has strong reusable IPC pieces, but they are not split cleanly.

What exists now:

- protocol/session/server logic in the current IPC crate
- a more complete native integration in `apps/spiders-wm/src/ipc.rs`
- a duplicated temporary native integration in `apps/spiders-wm-x`
- no real IPC abstraction yet in `apps/spiders-wm-www`

That causes:

- duplicated native glue
- drift risk between Wayland and X11
- no clean path for browser-side fake IPC

## End-State Crate Responsibilities

## `crates/ipc/core`

This crate should own:

- protocol types
- request/response/event envelopes
- codecs
- subscription topic matching
- per-client session state
- shared server-side request classification
- shared request resolution
- the minimal backend handler trait

This crate should not own:

- Unix sockets
- `calloop` registration
- browser APIs
- UI
- app-specific model access

## `crates/ipc/native`

This crate should own:

- Unix socket path setup and cleanup
- listener bind helpers
- client stream bookkeeping helpers
- `calloop` listener/client source registration helpers
- serve-one-client plumbing
- native event broadcast plumbing

This crate should depend on:

- `crates/ipc/core`
- `calloop`

This crate should not know about:

- Smithay
- X11 state
- Leptos
- preview UI

## `crates/ipc/browser`

This crate should own:

- browser-side in-memory IPC server/client plumbing
- browser-friendly request submission
- browser-friendly subscription delivery
- implementation using `web-sys` / `js-sys`
- a per-client `MessageChannel` / `MessagePort` transport model

This crate should not own:

- fake terminal UI
- panels
- text editors
- Leptos components

## Minimal Backend Trait

The shared backend trait should stay narrow.

Proposed shape:

```rust
pub trait IpcHandler {
    type Error;

    fn handle_query(&mut self, query: QueryRequest) -> Result<QueryResponse, Self::Error>;
    fn handle_command(&mut self, command: WmCommand) -> Result<(), Self::Error>;
    fn handle_debug(&mut self, request: DebugRequest) -> Result<DebugResponse, Self::Error>;
}
```

This trait should not include:

- transport
- loop registration
- socket paths
- UI hooks
- subscription storage

## Shared Request Resolution

The reusable request-routing logic currently living in `apps/spiders-wm/src/ipc.rs` should move into `crates/ipc/core`.

That shared helper should:

- accept `IpcServerState`
- accept a client id and request
- route immediate session responses internally
- invoke the handler trait for query / command / debug work
- produce the final `IpcResponse`

That becomes the one request resolution path for:

- Wayland
- X11
- browser fake IPC

## Native Architecture

The native IPC stack should be:

- `crates/ipc/core`
  - protocol, session, routing, handler trait
- `crates/ipc/native`
  - Unix transport plus `calloop` integration
- app crate
  - minimal handler implementation
  - backend-specific debug operations
  - runtime event production

## Why `calloop` For Both Native Backends

Using `calloop` for both native backends gives:

- one readiness model
- one listener/client registration model
- less backend-specific socket code
- fewer ad hoc idle polling heuristics
- better parity between Wayland and X11

X11 should not keep a separate polling-style IPC architecture.

## Browser Architecture

The browser should reuse:

- `IpcRequest`
- `IpcResponse`
- `WmCommand`
- `QueryRequest`
- `DebugRequest`
- `WmEvent`

But it should not emulate Unix sockets.

Instead, `crates/ipc/browser` should provide an in-memory browser-facing API built around `MessageChannel` and `MessagePort`.

## Chosen Browser Construct

Use:

- `web_sys::MessageChannel`
- `web_sys::MessagePort`
- `web_sys::MessageEvent`

Do not use:

- `BroadcastChannel`
- DOM UI callbacks as the transport layer
- direct app-state method calls from the fake terminal UI

## Why `MessageChannel`

`MessageChannel` is the best fit because it gives us:

- explicit per-client connections
- async message delivery
- a request/response/event shape that maps well to native IPC semantics
- support for multiple independent clients
- no dependence on global event names
- a transport abstraction that remains UI-free

It is a better fit than `BroadcastChannel`, because browser IPC here is not a cross-tab pubsub problem.

It is a better fit than `EventTarget`, because we want connection-oriented request/response semantics rather than a loose DOM event bus.

## Browser Transport Shape

The browser implementation should look like:

- a browser IPC server owns shared `IpcServerState`
- `connect()` creates a `MessageChannel`
- the server retains one `MessagePort`
- the caller receives a client wrapper around the peer `MessagePort`
- requests are posted as structured messages
- responses and subscribed events are posted back through the client port

That means the DOM terminal app is fake only in presentation.

Its IPC semantics should still be real within `spiders-wm-www`:

- queries should resolve through the shared IPC request path
- commands should mutate preview state through the shared IPC request path
- subscriptions should receive later `WmEvent` deliveries through the same IPC machinery

## Browser API Shape

The browser crate should expose a small programmatic API, not UI.

Recommended shape:

```rust
pub struct BrowserIpcServer<H> {
    // owns IpcServerState and browser connection state
}

pub struct BrowserIpcClient {
    // owns the client-side MessagePort and request tracking
}

impl<H: IpcHandler> BrowserIpcServer<H> {
    pub fn new(handler: H) -> Self;
    pub fn connect(&mut self) -> BrowserIpcClient;
    pub fn broadcast_event(&mut self, event: WmEvent);
}

impl BrowserIpcClient {
    pub async fn request(&self, request: IpcRequest) -> Result<IpcResponse, BrowserIpcError>;
    pub fn on_event(
        &self,
        handler: impl FnMut(IpcResponse) + 'static,
    ) -> BrowserIpcSubscription;
    pub fn close(&self);
}
```

The exact Rust API can change during implementation, but these semantics should hold:

- the server owns shared session state
- each client is a real logical IPC client
- request/response correlation is handled inside the browser crate
- event subscription delivery is handled inside the browser crate
- UI code should not manually decode raw browser transport messages

## Browser Delivery Rules

The browser crate should:

- serialize protocol messages using the shared IPC types
- convert them into browser message payloads internally
- preserve request ids and response ids
- deliver subscribed events only to matching clients
- support multiple independent clients simultaneously

## Browser UI Boundary

`apps/spiders-wm-www` should only need to do things like:

- create a browser IPC server around preview state
- create a client for the fake terminal
- submit requests from the terminal UI
- render responses and event output

It should not need to know about:

- `MessagePort` lifecycle details
- client id allocation
- subscription bookkeeping
- request correlation tables

The implementation can use `spawn_local`, `MessagePort` handlers, and `js-sys` / `web-sys` message plumbing internally, but those details should stay inside the crate.

The fake terminal in `apps/spiders-wm-www` should talk to that API.

## App Responsibilities

## `apps/spiders-wm`

- implement the minimal handler trait
- use `crates/ipc/native` for all native IPC transport work
- expose existing debug dump functionality through the trait
- push runtime events into the shared broadcast path

## `apps/spiders-wm-x`

- implement the minimal handler trait
- delete the temporary duplicated IPC transport logic
- use `crates/ipc/native` through `calloop`
- push runtime events into the shared broadcast path

## `apps/spiders-wm-www`

- implement the minimal handler trait over preview/session state
- use `crates/ipc/browser`
- build fake terminal / console UI locally in the app

## Design Rules

## 1. Wayland Is The Extraction Source

The native extraction source should be `apps/spiders-wm/src/ipc.rs`, because it is the most complete implementation.

## 2. Shared Crates Own Reusable Logic

If logic does not depend on:

- Smithay
- X11
- Leptos
- app-local state shape

it should not live in an app crate.

## 3. Browser Reuses Semantics, Not Native Transport

The browser should share protocol and routing semantics, not native socket mechanics.

## 4. App Integrations Should Be Thin

The apps should not own shared listener/client/session policy.

They should only answer IPC work and emit events.

## Proposed Refactor Phases

## Phase 1. Create `crates/ipc/core`

- move protocol/session/server/codec logic into `crates/ipc/core`
- move shared request resolution there
- add the minimal `IpcHandler` trait
- add focused tests for routing and subscriptions

## Phase 2. Create `crates/ipc/native`

- move shared native socket helpers there
- add shared `calloop` registration helpers there
- add shared native client serving and event broadcast helpers there

## Phase 3. Migrate Wayland

- switch `apps/spiders-wm` to the new `core` and `native` crates
- preserve current behavior
- keep app-local code limited to handler/debug/event integration

## Phase 4. Migrate X11

- remove the temporary X11 IPC transport code
- switch X11 to the new `core` and `native` crates
- integrate native IPC via `calloop`
- verify parity with Wayland request handling

## Phase 5. Create `crates/ipc/browser`

- implement browser-side in-memory IPC plumbing
- implement browser-side transport with `MessageChannel` / `MessagePort`
- implement browser-friendly async request and event delivery with `web-sys` / `js-sys`
- keep the crate UI-free

## Phase 6. Add Browser UI Integration

- wire `apps/spiders-wm-www` to `crates/ipc/browser`
- implement the fake terminal / fake IPC console in the app
- drive preview state through the shared IPC semantics

## Verification

We should verify at least:

## Shared Core

- query / command / debug routing
- session subscribe / unsubscribe behavior
- event topic matching
- error responses

## Native

- Wayland query/command/debug/event flows still work
- X11 query/command/debug/event flows work through the shared native stack
- subscribed clients remain connected while idle

## Browser

- fake IPC request path returns real `IpcResponse` values
- fake IPC commands mutate preview state correctly
- fake IPC subscriptions receive later events
- UI in `apps/spiders-wm-www` stays outside the browser IPC crate

## Recommendation

Start with:

1. `crates/ipc/core`
2. `crates/ipc/native`
3. Wayland migration
4. X11 migration to `calloop`
5. `crates/ipc/browser`
6. browser UI integration in `apps/spiders-wm-www`

That keeps extraction grounded in the most complete implementation, removes the temporary X11 path instead of polishing it, and gives the browser a real shared IPC surface without mixing infrastructure with UI.
