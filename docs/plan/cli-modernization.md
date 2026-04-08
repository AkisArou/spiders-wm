# CLI Modernization Plan

## Goals

- Rebuild `spiders-cli` as a maintained, discoverable command line interface.
- Use a single shared command model across:
  - native CLI
  - web CLI terminal
  - shell completions
- Reuse existing runtime types where possible instead of inventing a parallel command vocabulary.
- Keep browser terminal UX aligned with the native CLI syntax.

## Non-goals

- A top-level `web` CLI group. Browser build/dev flows are app tooling, not WM CLI surface.
- Exposing every internal runtime command directly as raw CLI syntax.
- Depending on `xterm.js` for command completion logic.

## Current state

`crates/cli/src/main.rs` currently parses `std::env::args()` manually. It detects commands with string presence checks like `ipc-query` or `check-config`, and reads options with ad hoc helpers like `arg_value(&args, "--socket")`.

This has several problems:

- no real subcommand tree
- weak help and discoverability
- no shared source of truth for completion metadata
- awkward browser reuse
- command naming has drifted over time

## Proposed command tree

Top-level groups:

- `config`
  - `discover`
  - `check`
  - `build`
- `wm`
  - `query <query>`
  - `command <command>`
  - `monitor [topic...]`
  - `debug dump <kind>`
  - `smoke`
- `completions`
  - `zsh`
  - `bash`
  - `fish`

Shared options:

- `--json`
- `--socket <path>` for WM IPC operations

Examples:

- `spiders-cli config discover`
- `spiders-cli config check`
- `spiders-cli config build`
- `spiders-cli wm query state`
- `spiders-cli wm query workspace-names`
- `spiders-cli wm command close-focused-window`
- `spiders-cli wm command cycle-layout-next`
- `spiders-cli wm monitor focus layout`
- `spiders-cli wm debug dump wm-state`
- `spiders-cli wm smoke`

## Shared source of truth

Create a new crate:

- `crates/cli/core`

This crate owns:

- command tree metadata
- token parsing
- completion suggestions
- help text fragments
- mapping from CLI tokens to runtime requests

This crate should be consumed by:

- `crates/cli` for native execution
- `apps/spiders-wm-www` for web terminal parsing and suggestions
- shell completion generation

## Runtime type reuse

We should reuse existing runtime types for the actual semantic payloads:

- `spiders_core::query::QueryRequest`
- `spiders_core::command::WmCommand`
- `spiders_ipc::IpcSubscriptionTopic`
- `spiders_ipc::DebugDumpKind`

The CLI core should wrap them in small metadata-aware types instead of exposing them raw.

Recommended wrapper types:

- `CliQuery`
- `CliCommand`
- `CliTopic`
- `CliDumpKind`

Each wrapper should provide:

- stable CLI name
- optional aliases
- short help text
- parsing from CLI token(s)
- conversion to runtime type
- completion candidates

This keeps the runtime semantics centralized while making CLI naming/documentation stable.

## Web terminal plan

The browser tab is now conceptually a `CLI` terminal, not an `IPC` terminal.

The web terminal should eventually accept the same command language as native `spiders-cli`, such as:

- `wm query state`
- `wm command cycle-layout-next`
- `wm monitor focus`

The browser adapter will:

- parse command input with `spiders-cli-core`
- execute supported `wm` commands over browser IPC
- render text output in `xterm.js`
- request suggestions/completions from `spiders-cli-core`

## xterm.js completion findings

`xterm.js` provides the terminal surface and official addons such as fit, search, attach, serialize, and web links.

It does not provide an official autocomplete or suggestion addon.

Package and docs review found:

- official docs list no autocomplete addon
- official addon catalog does not include command completion
- third-party addons exist for unrelated concerns like search bars, but there is no credible official completion addon to standardize on

Conclusion:

- implement completion ourselves in the application layer
- keep `xterm.js` as the terminal renderer/input source only
- drive `Tab` completion and suggestions from `spiders-cli-core`

## Shell completions

Shell completions should also be generated from the shared CLI model.

Repo placement:

- `crates/cli/completions/`

Preferred long-term path:

- `spiders-cli completions zsh`
- `spiders-cli completions bash`
- `spiders-cli completions fish`

Checked-in fallback scripts can live alongside that if needed.

## Implementation phases

### Phase 1: shared CLI core

- create `crates/cli/core`
- define wrapper metadata types for queries, commands, topics, and dump kinds
- define structured parsed command types
- implement token parser for the new command tree
- implement completion/suggestion API

### Phase 2: native CLI migration

- update `crates/cli` to use `spiders-cli-core`
- keep current command behavior but expose the new command tree
- optionally keep compatibility aliases for old flat commands during transition
- keep text/json reporting behavior

### Phase 3: shell completions

- add completion rendering from `spiders-cli-core`
- emit zsh/bash/fish scripts from the binary
- add checked-in zsh script if desired

### Phase 4: web CLI

- teach the browser terminal to parse full CLI commands with `spiders-cli-core`
- wire `Tab` completion and suggestion listing
- align output/help text with native CLI

## Initial scope

The first implementation pass should focus on the existing maintained surface only:

- `config discover`
- `config check`
- `config build`
- `wm query`
- `wm command`
- `wm monitor`
- `wm debug dump`
- `wm smoke`
- `completions zsh/bash/fish`

## Notes

- Do not add a separate web-only command language.
- Do not duplicate runtime enums as a second semantic model.
- Prefer a small, explicit CLI surface over exposing every `WmCommand` variant immediately.
