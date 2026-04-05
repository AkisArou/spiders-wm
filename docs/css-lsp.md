# CSS LSP

This document describes the architecture and project model for `spiders-css-lsp`.

It is the durable replacement for the temporary crate-local planning doc that lived at `crates/css-lsp/LSP.md` during implementation.

## Purpose

`spiders-css-lsp` provides editor features for the spiders CSS language and its project-aware integration with authored TSX layouts.

It is intentionally CSS-focused.

It is not a replacement for TypeScript or TSX language tooling.

## Ownership

`spiders-css` owns:

- CSS language metadata
- parser and compilation logic
- editor-agnostic analysis primitives
- source-ranged diagnostics and symbols that are also useful outside the LSP

`spiders-css-lsp` owns:

- LSP transport and protocol shaping
- document lifecycle
- project discovery and app scoping
- TSX selector indexing
- project-aware completion, diagnostics, hover, definition, references, rename, code actions, and workspace symbols

This keeps project/editor behavior out of `spiders-css` while still making `spiders-css` the single source of truth for the language itself.

## Project Model

The LSP follows the same app graph model as the runtime JS pipeline.

It reuses discovery and graph building from `spiders-runtime-js-core`.

The relevant scopes are:

- config app
- layout apps

### Config app

The config app is rooted at the nearest discovered `config.tsx`, `config.ts`, `config.jsx`, or `config.js`.

Its CSS surface is the root `index.css` alongside the config entry.

This is the global CSS scope.

### Layout app

Each layout app is rooted at a layout entry like `layouts/<name>/index.tsx`.

Its scope includes:

- the layout entry module
- imported authored TS and TSX modules in that app graph
- the layout `index.css`
- imported CSS in that app graph

This is the layout-local CSS scope.

Sibling layouts are intentionally isolated from each other.

## Selector Boundary Rules

Selector-aware features use app scope boundaries.

That means:

- root `index.css` sees selectors authored in the config app scope
- `layouts/master-stack/index.css` sees selectors authored in the `master-stack` app scope
- shared components imported by that layout are included in scope
- non-imported files outside the app graph are not included
- sibling layout selectors do not leak into each other

This applies to:

- selector completion
- unknown selector diagnostics
- selector hover
- go to definition
- find references
- rename
- selector code actions

## TSX Indexing

The LSP indexes authored selector data from TSX using `oxc`.

It currently extracts selector-relevant metadata from:

- `workspace`
- `group`
- `window`
- `slot`

It indexes:

- `id`
- `class`
- exact source ranges for static selector definitions

### Static extraction rules

Indexed with full confidence:

- string literals
- template literals without expressions
- static class segments inside helper-call patterns like `joinClasses("stack-group", growClass(...))`

Not indexed as renameable dynamic definitions:

- computed values whose final selector text cannot be determined statically
- dynamic helper return values
- interpolated template literals with expressions

The LSP is intentionally conservative here.

Static authored segments should be searchable and renameable.

Dynamic segments should degrade gracefully instead of producing unsafe rename results.

## Implemented Feature Surface

`spiders-css-lsp` currently provides:

- diagnostics from `spiders-css` shared analysis
- project-aware diagnostics for unknown selector ids and classes
- context-aware completion for CSS language constructs
- project-aware selector completion for known ids and classes
- hover for properties, pseudos, attribute keys, keyframes, and project-backed selectors
- document symbols
- workspace symbols for project-backed ids and classes
- definition for `animation-name` and selector ids/classes
- references for `animation-name` and selector ids/classes
- rename for `@keyframes`
- rename for selector ids/classes across CSS and TSX within scope
- quick-fix code actions for unknown selector ids/classes

## Non-Open File Coverage

Project-aware selector features are not limited to open files.

For scoped CSS navigation and rename, the LSP also uses stylesheet files discovered through the owning app graph.

That allows:

- references across scoped CSS files even when they are unopened
- rename across scoped CSS files even when they are unopened

## Ranking

Selector code actions and workspace symbol results use similarity-based ranking.

The ranking prefers:

- exact matches
- prefix matches
- token-start matches such as `stack` -> `stack-group`
- broader substring/subsequence matches after that

## Constraints

- `spiders-css` should not gain LSP-only project logic
- dependency versions stay owned by the workspace root
- the LSP should reuse runtime discovery/graph logic where possible
- dynamic selector semantics must remain conservative
- TS/TSX language semantics remain the responsibility of normal TypeScript tooling

## VS Code Workspace Setup

The VS Code extension client is intentionally opt-in.

It only activates in workspaces that contain a spiders config entry, and it still stays disabled until the workspace enables it.

Recommended setup for a local spiders config directory such as `~/.config/spiders-wm/`:

Create `~/.config/spiders-wm/.vscode/settings.json` with:

```json
{
  "spidersCss.enable": true,
  "css.validate": false
}
```

Notes:

- `spidersCss.enable` enables the custom spiders CSS LSP for that workspace
- `css.validate: false` reduces duplicate diagnostics from VS Code's built-in CSS validation
- if needed, the built-in `CSS Language Features` extension can also be disabled for that workspace from the Extensions view

## Future Work

Natural next directions include:

- broader safe static extraction patterns for selector indexing
- more code actions
- color/document color support
- docs validation against `spiders-css` metadata
- multi-root workspace support if needed
