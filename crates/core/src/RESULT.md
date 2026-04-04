# RESULT

Final architecture direction for the WM cleanup.

This file is the target design, not an intermediate migration note.

## Goals

1. Keep `apps/spiders-wm` and `apps/spiders-wm-www` lean and dumb.
2. Move shared WM behavior into `spiders-core` or `spiders-wm-runtime`.
3. Remove duplicate structs, duplicate helpers, and duplicate orchestration layers.
4. Prefer one canonical type per concept.
5. Do not keep compatibility layers.
6. Use one consistent flow model across native, web, and IPC.

## Final Mental Model

All meaningful data flow in the system should use the same shape.

Inputs:
- `WmCommand`
- `WmSignal`

Outputs:
- `WmEvent`
- `WmHostEffect`
- `QueryResponse`
- `StateSnapshot`

Boundary:
- `WmHost`

Runtime owner:
- `WmRuntime`

Meaning:
- `WmCommand` is user intent
- `WmSignal` is a host fact
- `WmRuntime` owns shared state transitions and orchestration
- `WmHostEffect` is the typed runtime-to-host effects protocol
- `WmHost` is the host adapter that receives `WmHostEffect`
- `WmEvent` describes facts that happened because state changed
- snapshots and query responses are read models for UI and IPC

This gives one mental model for:
- smithay WM
- web WM
- IPC
- future hosts

## Final Naming Decisions

These names should be treated as the target naming, not temporary naming.

### Core protocol names

- keep `WmCommand`
- add `WmSignal`
- rename `CompositorEvent` -> `WmEvent`
- keep `QueryRequest`
- keep `QueryResponse`

### Runtime names

- rename `WmEnvironment` -> `WmHost`
- add `WmHostEffect`
- keep `WmRuntime`
- remove `RuntimeCommand`
- remove `RuntimeResult`
- remove duplicate `CloseSelection` definitions and keep one canonical type if still needed

### Preview/runtime names

- remove `PreviewLayoutWindow`
- replace `PreviewLayoutWindow` and `PreviewSessionWindow` with one canonical type: `PreviewWindow`
- rename `PreviewSessionState` to `PreviewSession`

### App-local naming

- rename smithay render-local `WindowSnapshot` -> `WindowRenderSnapshot`

## Final Crate Boundaries

### `spiders-core`

Owns canonical domain and shared protocol types.

Owns:
- ids
- `WmModel` and related model structs
- focus, navigation, workspace, resize, layout domain
- `WmCommand`
- `WmSignal`
- `WmEvent`
- snapshots and read-model contracts
- query contracts
- pure projection helpers from `WmModel` into snapshots and query responses

Does not own:
- host integration
- JS runtime details
- UI state
- app orchestration

### `spiders-wm-runtime`

Owns the shared orchestrator layer for all hosts.

Owns:
- `WmRuntime`
- `WmHost`
- `WmHostEffect`
- shared command handling
- shared signal handling
- shared event emission policy
- shared query/read-model forwarding
- shared layout/session/snapshot helper logic
- preview session runtime state and reducers
- layout selection orchestration

Does not own:
- smithay rendering details
- leptos/editor-only state
- JS engine internals

### `runtimes/js`

Owns only JavaScript runtime implementation details.

Owns:
- JS module graph execution
- authored JS/JSX layout decoding
- runtime payload encode/decode
- prepared/authored config compilation and loading

Does not own:
- preview reducer logic
- host command handling
- host/session orchestration

### `apps/spiders-wm`

Owns smithay/native host concerns only.

Owns:
- compositor integration
- scene application
- frame sync
- native rendering
- native input collection
- host-specific quirks
- `WmHost` implementation for smithay

Does not own:
- shared action layer
- duplicate runtime facade
- shared model projection logic
- shared command semantics

### `apps/spiders-wm-www`

Owns web UI/presentation concerns only.

Owns:
- leptos UI state
- editor buffers/files/routes
- debug log / demo log presentation
- browser-specific rendering and event wiring
- `WmHost` implementation for web preview quirks

Does not own:
- shared preview reducers
- shared command semantics
- shared geometry extraction
- layout selection semantics
- shared session conversion logic

### `spiders-web-bindings`

Target decision: remove entirely.

Reason:
- it contains an old duplicate runtime/preview implementation
- its logic belongs in `spiders-wm-runtime` or `runtimes/js`
- if wasm exports are still needed, they should be thin forwarders only

## Final Flow Model

### Input side

There are exactly two meaningful inputs into the runtime.

#### `WmCommand`

Use for user intent.

Examples:
- focus window left
- cycle layout
- toggle floating
- set layout
- close focused window
- reload config

Sources:
- keybindings
- UI buttons
- IPC clients
- tests
- automation

#### `WmSignal`

Use for host-originated facts.

Examples:
- window created
- window destroyed
- window mapped/unmapped
- window title changed
- app id changed
- output added/changed
- pointer interacted with window
- hovered window changed

Sources:
- smithay callbacks
- browser-side preview host facts
- integration tests

### Runtime side

`WmRuntime` receives both `WmCommand` and `WmSignal`.

It is responsible for:
- mutating shared state
- running shared business logic
- deciding when host effects are required
- emitting `WmHostEffect` values into `WmHost`
- emitting `WmEvent`
- answering queries using shared read models

### Output side

#### `WmEvent`

Use for emitted facts about what changed.

Examples:
- focus changed
- layout changed
- workspace changed
- window floating changed
- window fullscreen changed
- config reloaded

#### Queries and snapshots

Use for reads.

Examples:
- full state snapshot
- current workspace
- current output
- focused window

#### `WmHostEffect`

Use for runtime-to-host effect requests.

Examples:
- spawn external command
- request quit
- reload config through host plumbing
- apply layout in a host-specific way if the platform requires it

`WmHostEffect` is not a state fact and not a query result.

It is an effect request.

## `WmHost`

Final decision: `WmEnvironment` should be renamed to `WmHost`.

Reason:
- the trait represents the host adapter
- "host" is more precise than "environment"
- it matches the mental model better: runtime core in the middle, host around it

`WmHost` is not an input protocol.

It is only the runtime-to-host effects port.

That means:
- host -> runtime uses `WmCommand` and `WmSignal`
- runtime -> host uses `WmHostEffect` delivered through `WmHost`

### Final trait shape

Final decision: use one effect entrypoint, not many ad hoc methods.

```rust
pub trait WmHost {
    fn on_effect(&mut self, effect: WmHostEffect);
}
```

Reason:
- one typed effect channel is more uniform
- it mirrors the input side better
- adding a new host effect means adding one enum variant, not growing trait surface unpredictably
- host implementations are forced to handle the effect enum explicitly
- it keeps the architecture message simple: commands/signals in, events/effects out

### Why not `on_spawn`, `on_quit`, `on_layout`, ...

We should not use many trait methods as the final design.

Reason:
- it spreads the output protocol across many ad hoc methods
- it grows the trait every time a new effect is introduced
- it is less symmetrical with `WmCommand` and `WmSignal`
- it is harder to reason about as a protocol

The right place for `on_*` style naming is inside the host implementation if desired, not in the shared runtime boundary.

### What belongs in `WmHost`

- effect handling only

### What does not belong in `WmHost`

- focus rules
- workspace selection rules
- layout cycling rules
- preview session rules
- geometry extraction rules
- command interpretation
- event emission policy

Those belong in `spiders-wm-runtime`.

### `WmHostEffect` examples

The exact enum can evolve, but the shape should be like this:

```rust
pub enum WmHostEffect {
    SpawnCommand { command: String },
    RequestQuit,
    ReloadConfig,
    ApplyLayout { layout_name: String },
}
```

The enum should stay focused on true host effects.

It should not absorb domain state changes that are already represented by `WmEvent`.

## Minimal Example

Minimal example of `apps/spiders-wm-www` talking to `spiders-wm-runtime`.

```rust
use spiders_core::command::{FocusDirection, WmCommand};
use spiders_core::effect::WmHostEffect;
use spiders_core::event::WmEvent;
use spiders_core::query::{QueryRequest, QueryResponse};
use spiders_core::signal::WmSignal;
use spiders_wm_runtime::{WmHost, WmRuntime};

struct WebHost {
    log: Vec<String>,
}

impl WmHost for WebHost {
    fn on_effect(&mut self, effect: WmHostEffect) {
        match effect {
            WmHostEffect::SpawnCommand { command } => {
                self.log.push(format!("spawn {command}"));
            }
            WmHostEffect::RequestQuit => {
                self.log.push("quit ignored in web".into());
            }
            WmHostEffect::ReloadConfig => {
                self.log.push("reload config".into());
            }
            WmHostEffect::ApplyLayout { layout_name } => {
                self.log.push(format!("apply layout {layout_name}"));
            }
        }
    }
}

fn on_key(runtime: &mut WmRuntime, host: &mut WebHost) {
    let events: Vec<WmEvent> = runtime.dispatch_command(
        host,
        WmCommand::FocusDirection {
            direction: FocusDirection::Right,
        },
    );

    let state: QueryResponse = runtime.query(QueryRequest::State);
    render(events, state);
}

fn on_browser_fact(runtime: &mut WmRuntime) {
    let events: Vec<WmEvent> = runtime.handle_signal(
        WmSignal::WindowTitleChanged {
            window_id: "win-1".into(),
            title: Some("Terminal".into()),
        },
    );

    let state: QueryResponse = runtime.query(QueryRequest::State);
    render(events, state);
}

fn render(events: Vec<WmEvent>, state: QueryResponse) {
    let _ = (events, state);
}
```

Meaning:
- web UI sends `WmCommand`
- web host forwards host facts as `WmSignal`
- runtime updates shared state
- runtime emits `WmEvent`
- web UI renders from snapshots/query responses
- web app never reimplements focus/layout/workspace business logic

The exact same flow should be true for smithay and IPC.

## IPC in the Same Model

IPC is not a separate architecture.

IPC is only transport for the same concepts:
- commands in as `WmCommand`
- queries in as `QueryRequest`
- events out as `WmEvent`
- reads out as `QueryResponse`

If remote signal injection is ever needed, it should use `WmSignal` explicitly rather than inventing a new transport-specific concept.

## Struct and Enum Decisions

### Keep separate

- `PreviewWindow` and `WindowSnapshot`
- `WmCommand` and `WmSignal`
- `WmEvent` and `WmHostEffect`

### Remove

- `PreviewLayoutWindow`
- `RuntimeCommand`
- `RuntimeResult`
- duplicate native action-layer ownership in `apps/spiders-wm`
- old duplicate preview/runtime structs in `spiders-web-bindings`

### Merge

- `PreviewSessionWindow` + `PreviewLayoutWindow` -> `PreviewWindow`

### Rename

- `WmEnvironment` -> `WmHost`
- `CompositorEvent` -> `WmEvent`
- app-local smithay render `WindowSnapshot` -> `WindowRenderSnapshot`
- `PreviewSessionState` -> `PreviewSession`

## `PreviewWindow` vs `WindowSnapshot`

Decision: do not merge them.

Reason:
- `WindowSnapshot` is the canonical domain/read-model snapshot from `spiders-core`
- `PreviewWindow` is editable runtime-side session state for non-compositor flows
- they are different concepts even if they overlap structurally

Correct relationship:
- `PreviewWindow` lives in `spiders-wm-runtime`
- `WindowSnapshot` lives in `spiders-core`
- shared conversion helpers convert `PreviewWindow` into `WindowSnapshot` when needed

## Helper Function Ownership

### Move to `spiders-core`

From `apps/spiders-wm/src/ipc.rs`:
- `state_snapshot_for_model`
- `query_response_for_model`
- `output_snapshot`
- `workspace_snapshot`
- `window_snapshot`
- `window_mode`

Reason:
- they are pure projections from `WmModel`
- they are not smithay-specific

### Move to `spiders-wm-runtime`

From `apps/spiders-wm-www/src/session.rs`:
- preview/runtime window -> `WindowSnapshot` conversion
- preview/runtime window -> layout evaluation input conversion
- snapshot geometry collection helpers
- empty/default preview geometry helper
- layout selection orchestration helpers

Reason:
- they operate on shared runtime concepts
- they are not web UI concerns

### Delete with `spiders-web-bindings`

Delete duplicate implementations of:
- preview reducer logic
- preview command handling
- JS layout preview decoding outside `runtimes/js`
- preview snapshot override/manipulation logic duplicated there

## Final Runtime API Shape

The runtime should converge on a small explicit surface.

```rust
impl WmRuntime {
    pub fn dispatch_command(&mut self, host: &mut impl WmHost, command: WmCommand) -> Vec<WmEvent>;
    pub fn handle_signal(&mut self, host: &mut impl WmHost, signal: WmSignal) -> Vec<WmEvent>;
    pub fn query(&self, request: QueryRequest) -> QueryResponse;
}
```

This is the final target API shape.

Any extra helper methods should support this model, not compete with it.

## Final Module Structure

### `crates/spiders-core/src/`

- `command.rs`
- `effect.rs`
- `signal.rs`
- `event.rs`
- `query.rs`
- `snapshot.rs`
- `wm/`

### `crates/spiders-wm-runtime/src/`

- `runtime.rs`
- `host.rs`
- `command_dispatch.rs`
- `query.rs`
- `preview/mod.rs`
- `preview/session.rs`
- `preview/layout.rs`
- `preview/snapshot.rs`
- `bindings.rs`
- `config.rs`

## WM App Simplification

### Native app

Target:
- `apps/spiders-wm` becomes a thin smithay host over `spiders-wm-runtime`

Delete or hollow out:
- `apps/spiders-wm/src/actions/facade.rs`
- `apps/spiders-wm/src/actions/window.rs`
- `apps/spiders-wm/src/actions/output.rs`
- `apps/spiders-wm/src/actions/seat.rs`

Keep:
- compositor code
- render code
- frame sync code
- smithay-specific host implementation

### Web app

Target:
- `apps/spiders-wm-www/src/session.rs` stops being a second runtime

Keep in web app:
- UI state
- editor/demo state
- browser rendering
- browser host implementation

Move out of web app:
- reducers
- shared command handling
- layout selection semantics
- geometry extraction
- preview/session conversion helpers

## Layout Ownership

Decision: layout switching belongs in `spiders-wm-runtime`, not in any specific WM app.

Reason:
- real native hosts need it too
- web preview needs it too
- the behavior is shared WM orchestration, not presentation

Implication:
- layout selection state is runtime-owned
- apps trigger it through `WmCommand`
- apps render the result

## Workspace Defaults

Decision:
- workspace naming defaults come from user config
- future default config can use `1`, `2`, `3`, ...

Implication:
- runtime should avoid opinionated baked-in workspace labels such as `1:dev`

## Execution Plan

### Phase 1

1. Rename `WmEnvironment` to `WmHost`.
2. Rename `CompositorEvent` to `WmEvent`.
3. Introduce `WmSignal` in `spiders-core`.
4. Split core protocol files into final names: `event.rs`, `query.rs`, `signal.rs`.
5. Move pure model projection helpers from native IPC into `spiders-core`.
6. Rename smithay render-local `WindowSnapshot` to `WindowRenderSnapshot`.

### Phase 2

1. Remove `RuntimeCommand`.
2. Remove `RuntimeResult`.
3. Expose the final runtime API:
4. `dispatch_command(&mut self, host: &mut impl WmHost, command: WmCommand) -> Vec<WmEvent>`
5. `handle_signal(&mut self, host: &mut impl WmHost, signal: WmSignal) -> Vec<WmEvent>`
6. `query(&self, request: QueryRequest) -> QueryResponse`

### Phase 3

1. Introduce `PreviewWindow` in `spiders-wm-runtime`.
2. Remove `PreviewLayoutWindow`.
3. Rename `PreviewSessionState` to `PreviewSession`.
4. Centralize preview/session conversion helpers in `spiders-wm-runtime`.
5. Move layout selection state into `spiders-wm-runtime`.

### Phase 4

1. Shrink `apps/spiders-wm-www/src/session.rs` into UI wrapper state only.
2. Route web actions into `WmCommand`.
3. Route web host facts into `WmSignal`.
4. Keep only browser-specific presentation and host behavior.

### Phase 5

1. Remove duplicate action/runtime layer from `apps/spiders-wm`.
2. Route smithay input/bindings into `WmCommand`.
3. Route smithay compositor facts into `WmSignal`.
4. Keep only smithay-specific host behavior.

### Phase 6

1. Delete `spiders-web-bindings`.

## Rules During Execution

Allowed:
- stubs where implementation is not finalized yet

Not allowed:
- new duplicate structs for the same concept
- app-local reimplementation of shared business logic
- keeping obsolete compatibility layers once the final owner exists

## Questions Resolved

### Should `PreviewLayoutWindow` be merged into `WindowSnapshot`?

No.

Correct direction:
- remove `PreviewLayoutWindow`
- use `PreviewWindow` as the runtime-owned editable session type
- convert to `WindowSnapshot` when a snapshot/read-model is needed

### Should `WmCommand` and `RuntimeCommand` be merged?

No.

Correct direction:
- keep `WmCommand` as the user-intent input protocol
- add `WmSignal` as the host-fact input protocol
- remove `RuntimeCommand`

### How should hosts communicate with `spiders-wm-runtime`?

Final answer:
- user intent goes in as `WmCommand`
- host facts go in as `WmSignal`
- host effects are requested as `WmHostEffect` and received through `WmHost::on_effect`
- changes come out as `WmEvent`
- reads come out as snapshots and query responses

### Should we add a `WmDispatcher` trait that returns `WmCommand`?

Final answer: no.

Reason:
- it duplicates the role of `WmRuntime::dispatch_command`
- it creates a second public command-entry abstraction
- it does not actually help ownership or flow clarity
- host-specific input translation can live in host/app code without becoming a shared trait

For Xorg or any future host, the enforcement should come from:
- implementing `WmHost`
- wiring platform callbacks into `WmSignal`
- wiring user actions into `WmCommand`

That is enough. A shared `WmDispatcher` trait is overkill.
