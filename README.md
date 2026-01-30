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
# Jump to a worktree. Create branch and worktree if missing.
cd "$(terris feature-a)"

# List worktrees
terris --list

# Delete a worktree
terris --delete feature-a
```

## How it works
- `terris <branch>` creates the worktree if missing and prints the path every time.
- If the branch exists, it is used directly.
- If the branch does not exist, it is created from the current HEAD.
- Default path is `~/.terris-worktrees/<repo-name>/<branch>-<random-key>`.

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.

## Name
The project is named after the [Terris people](https://coppermind.net/wiki/Terris), responsible for preserving the knowledge of the civilization.

## License
MIT
