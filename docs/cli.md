# CLI

The main entry point is `spiders-cli`.

Run commands with:

```bash
cargo run -p spiders-cli -- <command>
```

## Command Tree

Top-level groups:

- `config`
- `wm`
- `completions`

Global options:

- `--json`
- `--socket <path>` for `wm` IPC commands

## Config Commands

### `config discover`

Shows discovered authored and prepared config paths.

```bash
cargo run -p spiders-cli -- config discover
```

### `config check`

Validates config loading and layout modules.

```bash
cargo run -p spiders-cli -- config check
```

### `config build`

Builds or refreshes the prepared config cache.

```bash
cargo run -p spiders-cli -- config build
```

## WM Commands

### `wm smoke`

Exercises IPC framing and server handling without a live compositor.

```bash
cargo run -p spiders-cli -- wm smoke
```

### `wm query <query>`

Supported queries:

- `state`
- `focused-window`
- `current-output`
- `current-workspace`
- `monitor-list`
- `workspace-names`

Examples:

```bash
cargo run -p spiders-cli -- wm query state
cargo run -p spiders-cli -- wm query workspace-names --json
```

### `wm command <command>`

Examples:

```bash
cargo run -p spiders-cli -- wm command reload-config
cargo run -p spiders-cli -- wm command close-focused-window
cargo run -p spiders-cli -- wm command cycle-layout-next
cargo run -p spiders-cli -- wm command set-layout:columns
cargo run -p spiders-cli -- wm command select-workspace:2
```

### `wm monitor [topic...]`

Supported topics:

- `all`
- `focus`
- `windows`
- `workspaces`
- `layout`
- `config`

Examples:

```bash
cargo run -p spiders-cli -- wm monitor all
cargo run -p spiders-cli -- wm monitor workspaces layout --json
```

### `wm debug dump <kind>`

Supported dump kinds:

- `wm-state`
- `debug-profile`
- `scene-snapshot`
- `frame-sync`
- `seats`

Examples:

```bash
cargo run -p spiders-cli -- wm debug dump wm-state
cargo run -p spiders-cli -- wm debug dump scene-snapshot --json
```

## Shell Completions

Generate completions from the CLI:

```bash
cargo run -p spiders-cli -- completions zsh
cargo run -p spiders-cli -- completions bash
cargo run -p spiders-cli -- completions fish
```

Checked-in completion files live at:

- `crates/cli/completions/_spiders-cli`
- `crates/cli/completions/spiders-cli.bash`
- `crates/cli/completions/spiders-cli.fish`

### Install Zsh Completion

If your `fpath` already includes `~/.zsh/completions`:

```bash
mkdir -p ~/.zsh/completions
cp crates/cli/completions/_spiders-cli ~/.zsh/completions/
autoload -Uz compinit && compinit
```

If not, add this to `.zshrc`:

```bash
fpath=(~/.zsh/completions $fpath)
autoload -Uz compinit && compinit
```

### Install Bash Completion

```bash
mkdir -p ~/.local/share/bash-completion/completions
cp crates/cli/completions/spiders-cli.bash ~/.local/share/bash-completion/completions/spiders-cli
```

### Install Fish Completion

```bash
mkdir -p ~/.config/fish/completions
cp crates/cli/completions/spiders-cli.fish ~/.config/fish/completions/spiders-cli.fish
```

## Environment

Useful environment variables:

**Config Discovery**
- `SPIDERS_WM_HOME`
- `SPIDERS_WM_CONFIG_DIR`
- `SPIDERS_WM_CACHE_DIR`
- `SPIDERS_WM_AUTHORED_CONFIG`

**Runtime**
- `SPIDERS_WM_IPC_SOCKET`
- `SPIDERS_WM_DEBUG_PROFILE`
- `SPIDERS_WM_DEBUG_OUTPUT_DIR`

**Logging**
- `SPIDERS_LOG`
- `RUST_LOG`
