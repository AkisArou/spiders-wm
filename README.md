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

## Development

### Quick Start

1. Run `cargo check` to validate compilation.
2. Validate config with `cargo run -p spiders-cli -- check-config`.
3. Launch river with test config: `just dev` (or `SPIDERS_LOG=debug just dev` for verbose logging).
4. Use IPC tooling with `cargo run -p spiders-cli -- ipc-query --query state`.

### Testing

Run full test suite with `cargo test`. Individual crate tests:

```bash
cargo test -p spiders-wm
cargo test -p spiders-config
cargo test -p spiders-ipc
```

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

- `crates/spiders-wm` - river compositor integration + window management runtime
- `crates/spiders-config` - config loading, authoring layout evaluation, prepared config caching
- `crates/spiders-scene` - layout node scene graph and geometry resolution
- `crates/spiders-ipc` - IPC protocol, transport, and server
- `crates/runtimes/js` - JavaScript runtime bridge and config SDK surface
- `crates/spiders-cli` - CLI tooling for config validation, building, and IPC queries
- `crates/spiders-logging` - shared logging initialization and filter setup
- `crates/spiders-shared` - shared types and utilities
- `crates/spiders-tree` - tree/graph data structures for layout and window tracking

## Notes

- The old C repository is reference material only.
- User-facing terminology is `workspace` everywhere.
- JS config is evaluated in a restricted runtime: config and layout code do not receive raw compositor objects.
- Focus commands do not reorder the window stack (separate from window swap/move actions).
- Keybindings are deduplicated semantically (e.g., `alt+Return` and `Alt+Enter` map to the same underlying key and modifiers).
- Default bindings are provided if not configured: focus (hjkl), swap/move (Shift+hjkl), resize (Ctrl+hjkl), and workspace nav (1-9).
