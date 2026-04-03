# spiders-wm-web

This app is a Dioxus prototype for a Rust-first replacement of the current React playground.

The current prototype is intentionally narrow: it visualizes a few focus layouts and drives directional navigation through `spiders-core` directly. That makes it useful for evaluating whether Dioxus feels viable for layout and focus debugging before attempting a larger migration.

## What it does

- renders the A/B/C/D repro layout and the 5-window two-column layout
- uses `spiders-core::navigation::select_directional_focus_candidate`
- shows current scope path and remembered focus per scope
- keeps the whole focus loop in Rust instead of crossing into a JS state layer

## Run

From this directory:

```bash
dx serve
```

## Styling

- Edit [tailwind.css](/home/akisarou/projects/spiders-wm/apps/spiders-wm-web/tailwind.css) to change Tailwind theme/input scanning.
- Edit [main.css](/home/akisarou/projects/spiders-wm/apps/spiders-wm-web/assets/styling/main.css) for authored application styles.
- Do not hand-edit [tailwind.css](/home/akisarou/projects/spiders-wm/apps/spiders-wm-web/assets/tailwind.css). DX generates that file from the root Tailwind source file.
- Keep [tailwind.css](/home/akisarou/projects/spiders-wm/apps/spiders-wm-web/assets/tailwind.css) committed for now so cargo-only workflows and asset references stay stable.

## Next useful expansions

- feed the app from real snapshot data instead of static demo layouts
- expose scene tree and inferred visual scope tree side by side
- port one preview command path from `spiders-web-bindings` into native Dioxus state

Editor work is intentionally deferred until the preview and system port is further along.

