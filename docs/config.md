# Config

`spiders-wm` loads configuration from JavaScript or TypeScript and exposes a
small typed SDK under `spiders-wm/*`.

## Config Files

Authored config is discovered in this order:

- `~/.config/spiders-wm/config.ts`
- `~/.config/spiders-wm/config.js`

Prepared runtime output is written to:

- `~/.cache/spiders-wm/config.js`

You can override discovery with environment variables:

- `SPIDERS_WM_AUTHORED_CONFIG` - path to config.ts/config.js file
- `SPIDERS_WM_CACHE_DIR` - path to runtime output cache directory
- `SPIDERS_WM_CONFIG_DIR` - config search directory (default: `~/.config/spiders-wm`)
- `SPIDERS_WM_HOME` - home directory base (used if other vars not set)

## Top-Level Keys

`SpiderWMConfig` supports:

- `workspaces?: string[]`
- `options?: OptionsConfig`
- `layouts?: LayoutsConfig`
- `inputs?: InputsConfig`
- `rules?: RulesConfig`
- `bindings?: BindingsConfig`
- `autostart?: string[]`
- `autostart_once?: string[]`

## Minimal Example

```ts
import type { SpiderWMConfig } from "spiders-wm/config";

export default {
  workspaces: ["1", "2", "3", "4", "5"],
  layouts: () => <workspace>{/* layout tree */}</workspace>,
} satisfies SpiderWMConfig;
```

For details on layout JSX, bindings, and rules, see `configs/` in the template or test_config directories.

## Typical Example

```ts
import type { SpiderWMConfig } from "spiders-wm/config";

import { bindings } from "./config/bindings";
import { inputs } from "./config/inputs";
import { layouts } from "./config/layouts";

export default {
  workspaces: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
  options: {
    sloppyfocus: true,
  },
  inputs,
  layouts,
  bindings,
  autostart_once: ["waybar"],
} satisfies SpiderWMConfig;
```

## Options

`options` currently supports:

- `sloppyfocus?: boolean`
- `attach?: "after" | "before"`
- `mod_key?: string` - modifier key for default bindings (default: `"Alt"`)
- `layouts_dir?: string`
- `source_layouts_dir?: string`
- `snapshot_fadeout_ms?: number`
- `titlebar_font?: { regular_path?: string; bold_path?: string }`

`titlebar_font` lets the compositor titlebar renderer use explicit font files instead of only probing common Linux defaults.

The compositor titlebar renderer currently supports titlebar background, bottom border, text typography, top corner radii, and non-inset `box-shadow`. Shadow rendering is intentionally approximate: multiple non-inset shadows are drawn, rounded top corners are respected, and the clipped titlebar body is composited over the shadow layer.

Example:

```ts
options: {
  sloppyfocus: true,
  titlebar_font: {
    regular_path: "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    bold_path: "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
  },
}
```

## Bindings

Config can specify custom `bindings`. If not provided, default milestone bindings are generated based on `mod_key`:

**Default Focus Bindings** (non-reordering):

- `{mod_key}+h/j/k/l` - focus left/down/up/right
- `{mod_key}+ctrl+1-9` - toggle view workspace
- `{mod_key}+1-9` - view workspace

**Default Move/Swap Bindings**:

- `{mod_key}+Shift+h/j/k/l` - swap/move left/down/up/right
- `{mod_key}+Shift+1-9` - assign focused window to workspace

**Default Spawn/Utility Bindings**:

- `{mod_key}+Return` - spawn terminal (foot)
- `{mod_key}+q` - close focused window
- `{mod_key}+space` - cycle layout
- `{mod_key}+Shift+space` - toggle floating

Bindings are deduplicated by semantic signature (modifiers + key/button), so custom bindings override defaults automatically without conflicts.

## Bindings Format

Each binding specifies a trigger and action:

## Layout Selection

`layouts` is a selection object, not the authored JSX tree itself.

Supported keys:

- `default?: string`
- `per_workspace?: string[]`
- `per_monitor?: Record<string, string>`

Example:

```ts
import type { LayoutsConfig } from "spiders-wm/config";

export const layouts = {
  default: "master-stack",
  per_workspace: ["master-stack", "columns", "columns"],
  per_monitor: {
    "DP-1": "columns",
  },
} satisfies LayoutsConfig;
```

## Rules

Each rule can match and assign window behavior.

Supported rule fields:

- `app_id?: string`
- `title?: string`
- `workspaces?: string | number | Array<string | number>`
- `floating?: boolean`
- `fullscreen?: boolean`
- `monitor?: string | number`

Example:

```ts
rules: [
  { app_id: "pavucontrol", floating: true },
  { app_id: "slack", workspaces: "3" },
];
```

## Bindings Object

Bindings are declarative entries with a trigger and a command descriptor.

```ts
import * as commands from "spiders-wm/commands";
import type { BindingsConfig } from "spiders-wm/config";

export const bindings = {
  mod: "super",
  entries: [
    { bind: ["mod", "Return"], command: commands.spawn("foot") },
    { bind: ["mod", "h"], command: commands.focus_dir("left") },
    { bind: ["mod", "space"], command: commands.cycle_layout() },
    { bind: ["mod", "1"], command: commands.view_workspace(1) },
    { bind: ["mod", "shift", "1"], command: commands.assign_workspace(1) },
  ],
} satisfies BindingsConfig;
```

Binding keys use XKB keysym names. `mod` is an alias resolved from `bindings.mod`.

## Supported Commands

`spiders-wm/commands` currently exposes:

- `spawn(command)`
- `reload_config()`
- `focus_next()` and `focus_prev()`
- `focus_dir(direction)`
- `swap_dir(direction)`
- `resize_dir(direction)`
- `resize_tiled(direction)`
- `focus_mon_left()` and `focus_mon_right()`
- `send_mon_left()` and `send_mon_right()`
- `view_workspace(index)`
- `toggle_view_workspace(index)`
- `assign_workspace(index)`
- `toggle_workspace(index)`
- `toggle_floating()`
- `toggle_fullscreen()`
- `set_layout(name)`
- `cycle_layout()`
- `move(direction)`
- `resize(direction)`
- `kill_client()`

Workspace actions accept shortcut numbers `1` through `9` and resolve them to
`workspaces[index - 1]` from your config.

## Inputs

`inputs` is a map keyed by:

- `"*"`
- `"type:keyboard"`
- `"type:pointer"`
- `"type:touchpad"`
- `"type:touch"`
- an exact device identifier string

Supported input fields include:

- keyboard: `xkb_layout`, `xkb_model`, `xkb_variant`, `xkb_options`, `repeat_rate`, `repeat_delay`
- pointer and touchpad: `accel_profile`, `pointer_accel`, `left_handed`, `middle_emulation`
- touchpad extras: `natural_scroll`, `tap`, `drag_lock`, `dwt`

## Runtime API From Config

`spiders-wm/api` exposes:

- `events.on`, `events.once`, `events.off`
- `wm.spawn`, `wm.reloadConfig`, `wm.setLayout`, `wm.cycleLayout`
- `wm.viewWorkspace`, `wm.toggleViewWorkspace`
- `wm.toggleFloating`, `wm.toggleFullscreen`
- `wm.focusDirection`, `wm.closeWindow`
- `query.getState`, `query.getFocusedWindow`, `query.getCurrentMonitor`, `query.getCurrentWorkspace`

Event names are:

- `focus-change`
- `window-created`
- `window-destroyed`
- `window-workspace-change`
- `window-floating-change`
- `window-fullscreen-change`
- `workspace-change`
- `layout-change`
- `config-reloaded`
