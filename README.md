# spiders-wm

`spiders-wm` is a Rust-native Wayland window management stack built around
JavaScript or TypeScript configuration, structural JSX layouts, CSS-based layout
styling, and CSS-based visual effects.

The rewrite targets:

- `river` for compositor integration
- `taffy` for layout computation
- `rquickjs` for the embedded JavaScript runtime
- `keyframe` for animation timelines

## What It Keeps

- keyboard-first window management
- named workspaces
- declarative bindings and window rules
- JSX layout trees built from `workspace`, `group`, `window`, and `slot`
- structural layout CSS for geometry
- effects CSS for window chrome and workspace transitions
- local IPC for queries, actions, and subscriptions

## Docs

- `docs/config.md` - config shape, rules, bindings, and examples
- `docs/css.md` - supported layout CSS and effects CSS
- `docs/jsx.md` - JSX layout elements, props, matching, and examples
- `docs/ipc.md` - IPC transport, queries, actions, and events
- `docs/cli.md` - CLI commands and examples

## Quick Start

1. Run `cargo check`.
2. Validate config with `cargo run -p spiders-cli -- check-config`.
3. Build prepared config with `cargo run -p spiders-cli -- build-config`.
4. Use IPC tooling with `cargo run -p spiders-cli -- ipc-query --query state`.

## Configuration At A Glance

The default authored config lives at `~/.config/spiders-wm/config.ts` or
`~/.config/spiders-wm/config.js`.

Minimal example:

```ts
import type { SpiderWMConfig } from "spiders-wm/config";
import { bindings } from "./config/bindings";
import { layouts } from "./config/layouts";

export default {
  workspaces: ["1", "2", "3", "4", "5"],
  layouts,
  bindings,
} satisfies SpiderWMConfig;
```

## Repository Layout

- `crates/spiders-river` - river-focused compositor/runtime integration
- `crates/spiders-config` - config loading and prepared config handling
- `crates/spiders-scene` - scene validation, styling, and geometry pipeline
- `crates/spiders-ipc` - IPC protocol, transport, and server helpers
- `crates/runtimes/js` - JavaScript runtime bridge and SDK surface
- `crates/spiders-cli` - local development and IPC CLI

## Notes

- The old C repository is reference material only.
- User-facing terminology is `workspace` everywhere.
- JS stays capability-limited: config and layout code do not receive raw compositor objects.
