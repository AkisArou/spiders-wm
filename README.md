# spiders-wm

`spiders-wm` is a Rust tiling window-manager built around web technologies.
TypeScript config, JSX layouts, CSS.

Today the repo contains three primary app surfaces:

- `spiders-wm`: the Smithay-based Wayland compositor host, currently used as a nested compositor by default and with TTY-preview work in progress
- `spiders-wm-x`: an X11/XCB host used for development and experimentation
- `spiders-wm-www`: a browser preview and authoring surface for config, layouts, and scene output

Core implementation pieces include:

- `rquickjs` for the native JS runtime
- `oxc` for transpiling jsx
- `taffy` for layout computation
- `Smithay` for Wayland compositor integration

## Configuration At A Glance

Authored config is discovered from:

- `~/.config/spiders-wm/config.ts`:

```ts
import type { SpiderWMConfig } from "@spiders-wm/sdk/config";

import { bindings } from "./config/bindings.ts";
import { inputs } from "./config/inputs.ts";
import { layouts } from "./config/layouts.ts";

export default {
  workspaces: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
  options: {
    sloppyfocus: true,
    attach: "after",
  },
  inputs,
  layouts,
  rules: [],
  bindings,
} satisfies SpiderWMConfig;
```

Example `config/bindings.ts`:

```ts
import * as commands from "@spiders-wm/sdk/commands";
import type { BindingsConfig } from "@spiders-wm/sdk/config";

export const bindings: BindingsConfig = {
  mod: "alt",
  entries: [
    { bind: ["mod", "Return"], command: commands.spawn("foot") },
    { bind: ["mod", "d"], command: commands.spawn("rofi -show drun") },
    { bind: ["mod", "h"], command: commands.focus_dir("left") },
    { bind: ["mod", "j"], command: commands.focus_dir("down") },
    { bind: ["mod", "k"], command: commands.focus_dir("up") },
    { bind: ["mod", "l"], command: commands.focus_dir("right") },
    { bind: ["mod", "shift", "h"], command: commands.swap_dir("left") },
    { bind: ["mod", "shift", "j"], command: commands.swap_dir("down") },
    { bind: ["mod", "shift", "k"], command: commands.swap_dir("up") },
    { bind: ["mod", "shift", "l"], command: commands.swap_dir("right") },
    { bind: ["mod", "space"], command: commands.cycle_layout() },
    { bind: ["mod", "shift", "space"], command: commands.toggle_floating() },
    { bind: ["mod", "q"], command: commands.kill_client() },
    { bind: ["mod", "1"], command: commands.view_workspace(1) },
    { bind: ["mod", "2"], command: commands.view_workspace(2) },
    { bind: ["mod", "3"], command: commands.view_workspace(3) },
    { bind: ["mod", "shift", "1"], command: commands.assign_workspace(1) },
    { bind: ["mod", "shift", "2"], command: commands.assign_workspace(2) },
    { bind: ["mod", "shift", "3"], command: commands.assign_workspace(3) },
  ],
};
```

Example `config/layouts.ts`:

```ts
import type { LayoutsConfig } from "@spiders-wm/sdk/config";

export const layouts: LayoutsConfig = {
  default: "master-stack",
  per_workspace: ["master-stack", "primary-stack"],
  per_monitor: {
    "eDP-1": "master-stack",
  },
};
```

Example `layouts/master-stack/index.tsx`:

```tsx
import type { LayoutContext } from "@spiders-wm/sdk/layout";

import "./index.css";

export default function layout(ctx: LayoutContext) {
  return (
    <workspace id="frame" class="playground-workspace">
      <slot id="master" take={1} class="master-slot" />

      {ctx.windows.length > 1 ? (
        <group id="stack" class="stack-group">
          <slot id="stack-slot" class="stack-group__item" />
        </group>
      ) : null}
    </workspace>
  );
}
```

Example `layouts/master-stack/index.css`:

```css
#frame {
  display: flex;
  flex-direction: row;
  gap: 6px;
  padding: 6px;
  width: 100%;
  height: 100%;
}

.master-slot {
  flex-basis: 0;
  flex-grow: 3;
  min-width: 0;
  min-height: 0;
}

#stack {
  border-color: #2f3647;
}

.stack-group {
  display: flex;
  flex-direction: column;
  gap: 10px;
  flex-basis: 0;
  flex-grow: 2;
  min-width: 0;
}

.stack-group__item {
  flex-basis: 0;
  flex-grow: 1;
  min-height: 0;
  border-color: #2f3647;
}
```

See `docs/config.md`, `docs/jsx.md`, `docs/css.md`, `template/`, and
`test_config/` for the fuller authored workflow.

## What It Provides

- keyboard-first workspace management
- named workspaces, bindings, and window rules
- JSX layout trees built from `workspace`, `group`, `window`, and `slot`
- spiders CSS for layout and compositor-managed presentation
- local IPC for queries, commands, debug dumps, and subscriptions
- browser and editor tooling around the authored config/layout workflow

## Docs

- `docs/config.md` - config discovery, top-level shape, bindings, rules, inputs, and examples
- `docs/css.md` - supported spiders CSS selectors, properties, and runtime notes
- `docs/jsx.md` - layout JSX elements, matching, context, and examples
- `docs/ipc.md` - socket transport, queries, actions, debug dumps, and events
- `docs/cli.md` - current `spiders-cli` command tree and examples
- `docs/css-lsp.md` - CSS language server architecture, scope model, and editor setup
- `docs/titlebar.md` - historical note on removed titlebar-specific runtime work

## Repository Layout

- `apps/spiders-wm` - Smithay-based Wayland compositor host and runtime integration
- `apps/spiders-wm-x` - X11/XCB host for experimentation and management flows
- `apps/spiders-wm-www` - browser preview/authoring app built with Leptos
