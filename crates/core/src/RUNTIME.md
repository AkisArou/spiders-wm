# RUNTIME

Final runtime and authored-config architecture direction.

This file is the target design, not an intermediate note.

## Problems

1. `spiders-wm-runtime` currently depends on a concrete JS runtime implementation.
2. `apps/spiders-wm-www` reaches `rquickjs` transitively through `spiders-wm-runtime`.
3. Config loading and layout evaluation are conflated behind one broad runtime contract.
4. We want future config languages like Lua or Rust, not only TypeScript.
5. We want native and web hosts to inject the runtime they actually use instead of hardwiring QuickJS.

## Goals

1. Keep `spiders-wm-runtime` free of concrete runtime engines.
2. Support multiple authored config languages.
3. Support multiple layout evaluation engines.
4. Let hosts inject the runtime backend they need.
5. Keep browser builds free of QuickJS.
6. Preserve the current ability for `apps/spiders-wm-www` to model a full `~/.config/spiders-wm` project.

## Non-Goals

1. Do not keep compatibility layers for the current runtime API.
2. Do not require every future language runtime to exist immediately.
3. Do not force config language and layout language to always be the same implementation if we do not need that.

## Answers

### Should `spiders-config` move out of `spiders-wm-runtime`?

Not completely.

`spiders-wm-runtime` can still depend on `spiders_config::model::Config` as input data.

What must move out of `spiders-wm-runtime` is:
- config path discovery
- authored config loading
- prepared config cache refresh/rebuild
- construction of concrete config/runtime services
- any direct dependency on `spiders-runtime-js` or other runtime backends

So the decision is:
- keep `Config` as shared data passed into runtime
- remove config-runtime ownership from `spiders-wm-runtime`

### Should runtime selection depend on the user config?

Yes, but not by parsing the config first.

Runtime selection must happen before the config can be loaded.

That means selection should use project metadata that is readable without evaluating the config, for example:
- config entry filename or extension
- an explicit sidecar manifest later if needed

Initial selection rules should be simple:
- `config.ts`, `config.tsx`, `config.js`, `config.jsx`, `config.mjs`, `config.cjs` => JS runtime family
- `config.lua` => Lua runtime family
- future `config.rs` or equivalent => Rust runtime family

The authored config entry decides the config runtime.

## Final Mental Model

There are two separate runtime concerns.

### 1. Authored Config Runtime

This runtime knows how to:
- load authored config source
- load prepared config artifacts
- refresh or rebuild prepared config cache
- produce canonical `Config`

This is language-facing.

Examples:
- TypeScript/JS config runtime
- Lua config runtime
- Rust config runtime

### 2. Prepared Layout Runtime

This runtime knows how to:
- prepare a selected layout for a workspace
- build layout evaluation context
- evaluate a prepared layout into `SourceLayoutNode`

This is layout-engine-facing.

Examples:
- QuickJS layout evaluator
- browser JS module evaluator
- Lua layout evaluator
- Rust-native layout evaluator

These two runtime concerns may be implemented by the same backend crate, but they are not the same abstraction.

## Final Contracts

### Keep in `spiders-core`

`spiders-core` should own only layout/runtime contracts needed by shared WM logic.

Keep:
- `PreparedLayoutRuntime`
- `LayoutModuleContract`
- `LayoutEvaluationContext`
- `PreparedLayout`
- runtime error/read-model types used by shared logic

Do not put authored-config loading traits in `spiders-core`.

### Add in `spiders-config`

`spiders-config` should own authored-config runtime contracts because they produce `Config` and operate on config paths.

Add a trait roughly like:

```rust
pub trait AuthoringConfigRuntime: std::fmt::Debug {
    fn load_authored_config(&self, path: &Path) -> Result<Config, ConfigRuntimeError>;
    fn load_prepared_config(&self, path: &Path) -> Result<Config, ConfigRuntimeError>;
    fn refresh_prepared_config(
        &self,
        authored: &Path,
        prepared: &Path,
    ) -> Result<RuntimeRefreshSummary, ConfigRuntimeError>;
    fn rebuild_prepared_config(
        &self,
        authored: &Path,
        prepared: &Path,
    ) -> Result<RuntimeRefreshSummary, ConfigRuntimeError>;
}
```

`AuthoringLayoutService` should be reworked to depend on:
- `AuthoringConfigRuntime` for config loading/cache refresh
- `PreparedLayoutRuntime<Config = Config>` for layout preparation/evaluation

If a concrete backend implements both, the host can pass the same value for both.

For browser and other in-memory hosts, `spiders-config` may also provide an async source-bundle variant of the same service shape.
That companion service should own:
- config loading from in-memory source bundles
- prepared-layout loading from in-memory source bundles
- async layout evaluation for hosts where module execution is naturally async

## Final Injection Model

Hosts inject concrete runtimes.

### `apps/spiders-wm`

Native host should inject:
- a native authored config runtime
- a native prepared layout runtime

Initially both should come from the QuickJS-backed JS runtime family.

### `apps/spiders-wm-www`

Web host should inject:
- a browser-authored config runtime for the fixture project and future in-browser full-config editing
- a browser prepared layout runtime

The web host should not depend on QuickJS.

Because browser module evaluation is async and works from in-memory sources, the web host may use an async source-bundle service in `spiders-config` instead of the native sync file-path service.

## Final Crate Boundaries

### `spiders-core`

Owns:
- shared WM domain
- layout/runtime contracts used by shared WM logic
- layout evaluation context and prepared layout types

Does not own:
- config loading
- config language selection
- concrete JS/Lua/Rust engines

### `spiders-config`

Owns:
- `Config`
- config path discovery
- authored/prepared config service layer
- authored config runtime trait(s)
- config runtime selection policy

Does not own:
- WM host orchestration
- concrete QuickJS/browser engine details

### `spiders-wm-runtime`

Owns:
- WM runtime
- host/effect protocol
- preview/session reducers
- shared event emission
- shared layout selection orchestration

Does not own:
- config discovery or loading
- authored config service construction
- concrete runtime backends

It may still accept `Config` as input.

### `runtimes/js/core`

New crate or subcrate path.

Owns only JS runtime pieces that are wasm-safe or engine-neutral.

Owns:
- JS/TS source graph compilation
- module graph types
- runtime payload encoding/decoding
- browser-safe source rewriting and graph preparation helpers

Does not own:
- QuickJS
- authored config evaluation through QuickJS

### `runtimes/js/native`

New crate or subcrate path.

Owns native QuickJS-backed implementation details.

Owns:
- `rquickjs`
- QuickJS-backed authored config runtime
- QuickJS-backed prepared layout runtime

### `runtimes/js/browser`

Likely new crate or subcrate path. It could begin as web-app local glue, but the target home should be under the JS runtime family.

Owns:
- browser-side JS module graph evaluation
- browser-side authored config loading for full project config support
- browser-side prepared layout runtime for source-bundle evaluation

This may depend on wasm/web APIs and should stay QuickJS-free.

### Future runtime crates

Examples:
- `runtimes/lua`
- `runtimes/rust-config`

Each can implement one or both runtime contracts.

## Runtime Selection

Selection should happen in `spiders-config`, not in `spiders-wm-runtime`.

Add a small project-level selector, for example:

```rust
pub enum RuntimeKind {
    JavaScript,
    Lua,
    Rust,
}
```

Selection input should be:
- `ConfigPaths`
- authored config entry path
- maybe future explicit manifest override

Not the parsed `Config`, because parsing requires the runtime already.

## Recommended Service Shape

The host should resolve a runtime bundle and pass it into the service layer.

For example:

```rust
pub struct RuntimeBundle<C, L> {
    pub config_runtime: C,
    pub layout_runtime: L,
}
```

For JS on native, both may be QuickJS-backed.

For JS in browser:
- config runtime may use browser JS evaluation
- layout runtime may use browser JS module evaluation
- the host may assemble these behind an async source-bundle service instead of the native sync file-path service

## Browser Implication

`apps/spiders-wm-www` already models a full project fixture under `fixtures/spiders-wm/`.

That means the web target should be treated as a real authored-config consumer, not only a layout-preview toy.

So the target design should explicitly support:
- full authored config loading in browser for JS projects
- full layout module evaluation in browser for JS projects
- config/runtime service assembly in `spiders-config` for in-memory source bundles

Without depending on QuickJS.

## Migration Plan

1. Add this architecture document.
2. Introduce authored config runtime trait(s) in `spiders-config`.
3. Rework `AuthoringLayoutService` to depend on injected config runtime and layout runtime.
4. Remove config discovery/loading helpers from `spiders-wm-runtime`.
5. Stop re-exporting concrete runtime backends from `spiders-wm-runtime`.
6. Split current `runtimes/js` runtime family into:
   - `runtimes/js/core`
   - `runtimes/js/native`
   - optionally `runtimes/js/browser`
7. Move QuickJS-only code and dependencies into `runtimes/js/native`.
8. Keep wasm-safe JS graph/types/helpers in `runtimes/js/core`.
9. Update `apps/spiders-wm` to inject the QuickJS JS runtime bundle.
10. Update `apps/spiders-wm-www` to inject browser JS runtime adapters.
11. Remove any remaining `spiders-runtime-js` dependency from `spiders-wm-runtime`.
12. Verify `trunk serve` builds without trying to compile `rquickjs`.

Resulting implementation note:
- native uses a sync file-path-based authoring layout service
- browser uses an async source-bundle-based authoring layout service
- both are owned by `spiders-config`
- both produce canonical `Config` and canonical `SourceLayoutNode`

## Decision Summary

Final decisions:
- `spiders-wm-runtime` should not depend on concrete runtime backends
- authored config runtime and prepared layout runtime are separate abstractions
- runtime selection belongs to `spiders-config` or a config-facing service layer
- selection should use config entry metadata, not parsed config contents
- web should support full config projects, not only isolated layouts
- QuickJS should be native-only and must not be in the wasm dependency graph

## Open Questions

These should be answered during implementation, not by changing the direction above.

1. Should `runtimes/js/browser` be a standalone crate or just web-app-local glue first?
2. Do we want a sidecar manifest later for explicit runtime selection, or are filename/extension rules enough?
3. Should one backend be required to implement both contracts, or should hosts be free to mix config and layout runtimes?

Current recommendation:
- allow hosts to mix them if needed
- start with extension-based selection
- keep browser runtime under `runtimes/js/browser` and let `spiders-config` own service assembly
