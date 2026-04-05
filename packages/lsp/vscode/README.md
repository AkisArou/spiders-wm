# Spiders CSS

VS Code client for `spiders-css-lsp`.

## Activation

The extension only activates in workspaces that contain a spiders config entry:

- `config.tsx`
- `config.ts`
- `config.jsx`
- `config.js`

Even in those workspaces, the language server is disabled by default.

## Enabling It Manually

Open your spiders config workspace, for example:

- `~/.config/spiders-wm/`

Then create `.vscode/settings.json` in that workspace and opt in:

```json
{
  "spidersCss.enable": true,
  "css.validate": false
}
```

This file should live at:

- `~/.config/spiders-wm/.vscode/settings.json`

Recommended notes:

- `spidersCss.enable` turns this extension on for the workspace
- `css.validate: false` reduces duplicate diagnostics from VS Code's built-in CSS support

If you want to disable VS Code's built-in CSS language features more aggressively, there is not a reliable workspace settings key for fully turning that built-in extension off.

The practical option is:

1. Open the Extensions view.
2. Find `CSS Language Features`.
3. Choose `Disable (Workspace)`.

For most use cases, `css.validate: false` is the lowest-friction setting to start with.

## Development

The extension currently ships a bundled Linux x64 server binary and otherwise falls back to workspace-built binaries.

Resolution order:

- `server/linux-x64/spiders-css-lsp`
- `target/debug/spiders-css-lsp`
- `target/release/spiders-css-lsp`

You can also set an explicit path with:

- `spidersCss.server.path`

The extension prefers a bundled platform binary when one is present.

## Packaging

Build a `.vsix` with:

```sh
pnpm --filter spiders-css-lsp-vscode prepare:linux-x64
pnpm --filter spiders-css-lsp-vscode package
```

The icon is generated from `assets/spiders-wm-mark.svg` using `rsvg-convert`.
