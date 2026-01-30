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
From crates.io (once published):
```bash
cargo install terris
```

From source:
```bash
cargo install --path .
```

Update:
```bash
cargo install --path . --force
```

## Usage
```bash
terris <branch>
terris --list
terris --delete <branch>
```

## How it works
- `terris <branch>` creates the worktree if missing and prints the path every time.
- If the branch exists, it is used directly.
- If the branch does not exist, it is created from the current HEAD.
- Default path is `~/.terris-worktrees/<repo-name>/<branch>-<8-random-lowercase-letters>`.

## Examples
```bash
# Create or open a worktree (branch "feature-a")
terris feature-a

# List worktrees
terris --list

# Jump to a worktree
cd "$(terris feature-a)"

# Delete a worktree
terris --delete feature-a
```

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.

## Name
The project is named after the [Terris people](https://coppermind.net/wiki/Terris), responsible for preserving the knowledge of the civilization.

## License
MIT
