# terris

[![Crates.io](https://img.shields.io/crates/v/terris.svg)](https://crates.io/crates/terris) [![Docs.rs](https://img.shields.io/docsrs/terris.svg)](https://docs.rs/terris) [![CI](https://github.com/mfornet/terris/actions/workflows/ci.yml/badge.svg)](https://github.com/mfornet/terris/actions/workflows/ci.yml) [![License](https://img.shields.io/crates/l/terris.svg)](LICENSE)

A small, friendly Git worktree manager for everyday use.


## What it does
- Create new worktrees quickly (or jump to existing ones)
- List existing worktrees
- Remove worktrees safely
- Print a worktree path for `cd "$(terris <branch>)"`
- Simplify workflows with autonomous agents that work in the terminal in the same repository

## Install
From crates.io:
```bash
cargo install terris
```

From source:
```bash
git clone https://github.com/mfornet/terris.git
cd terris
cargo install --path .
```

## Usage

```bash
# Jump to a worktree.
cd "$(terris feature-a)"

# List worktrees
terris

# List all worktrees (including detached)
terris --all

# Delete a worktree
terris --rm feature-a
```


## How it works
- `terris <branch>` creates the worktree and prints the path every time.
- `terris` lists worktrees for the current repository.
- `terris --all` lists all worktrees, including ones without branches.
- If the branch exists, it is used directly.
- If the branch does not exist, behavior depends on your terris config.
- Default path is `~/.terris-worktrees/<repo-name>/<branch>-<random-key>`.

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.

## Configuration

terris reads configuration from these files, in this order:

1. `~/.terris/terris.toml` for user-wide defaults
2. `<git-root>/.terris.toml` for project-specific overrides

If both files exist, terris merges them and the project-local file overrides the global one. If neither exists, built-in defaults are used.

Create a global config:

```bash
mkdir -p ~/.terris
$EDITOR ~/.terris/terris.toml
```

Create a project-local config:

```bash
$EDITOR .terris.toml
```

Example:

```toml
[worktrees]
base_dir = "~/src/worktrees"
use_random_suffix = true
suffix_length = 8

[behavior]
on_missing_branch = "fetch, create"
auto_prune = true

[display]
show_head = true
```

### Available settings

#### `[worktrees]`

- `base_dir`: Base directory for new worktrees. Supports `~`. Default: `~/.terris-worktrees`
- `use_random_suffix`: Whether to append a random suffix to worktree directory names. Default: `true`
- `suffix_length`: Length of the random suffix when `use_random_suffix = true`. Default: `8`. Valid range: `1..=64`

With defaults, terris creates worktrees under:

```text
~/.terris-worktrees/<repo-name>/<branch>-<random-key>
```

If `use_random_suffix = false`, the path becomes:

```text
~/.terris-worktrees/<repo-name>/<branch>
```

#### `[behavior]`

- `on_missing_branch`: What terris should do when the requested branch does not exist locally. Default: `"error"`
- `auto_prune`: Run `git worktree prune` before listing worktrees. Default: `false`

`on_missing_branch` accepts a comma-separated list of actions tried in order:

- `error`: Fail immediately. This cannot be combined with other actions.
- `fetch`: Run `git fetch origin <branch>` and use the remote branch if found.
- `create`: Create a new local branch from the current `HEAD`.

Examples:

```toml
[behavior]
on_missing_branch = "error"
```

```toml
[behavior]
on_missing_branch = "fetch"
```

```toml
[behavior]
on_missing_branch = "fetch, create"
```

```toml
[behavior]
on_missing_branch = "create"
```

#### `[display]`

- `show_head`: Show the short `HEAD` commit hash as an extra column when listing worktrees. Default: `false`

## Shell completion

Generate a completion script and source it in your shell:

```bash
# Bash
terris --completions bash > /tmp/terris.bash
source /tmp/terris.bash

# Zsh
terris --completions zsh > /tmp/_terris
fpath=(/tmp $fpath)
autoload -U compinit && compinit

# Fish
terris --completions fish > /tmp/terris.fish
source /tmp/terris.fish
```

Install permanently (recommended):

```bash
# Bash (user-level)
mkdir -p ~/.local/share/bash-completion/completions
terris --completions bash > ~/.local/share/bash-completion/completions/terris

# Bash (system-wide)
sudo mkdir -p /etc/bash_completion.d
sudo terris --completions bash > /etc/bash_completion.d/terris

# Zsh
mkdir -p ~/.zsh/completions
terris --completions zsh > ~/.zsh/completions/_terris
fpath=(~/.zsh/completions $fpath)
autoload -U compinit && compinit

# Fish
mkdir -p ~/.config/fish/completions
terris --completions fish > ~/.config/fish/completions/terris.fish
```

## Name
The project is named after the [Terris people](https://coppermind.net/wiki/Terris), responsible for preserving the knowledge of the civilization.

## License
MIT
