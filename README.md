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
# Jump to a worktree. Branch must exist.
cd "$(terris feature-a)"

# List worktrees
terris

# List all worktrees (including detached)
terris --all

# Delete a worktree
terris --rm feature-a
```


## How it works
- `terris <branch>` creates the worktree (branch must exist) and prints the path every time.
- `terris` lists worktrees for the current repository.
- `terris --all` lists all worktrees, including ones without branches.
- If the branch exists, it is used directly.
- If the branch does not exist, the command fails with an error.
- Default path is `~/.terris-worktrees/<repo-name>/<branch>-<random-key>`.

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.

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
