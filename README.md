# terris

[![Crates.io](https://img.shields.io/crates/v/terris.svg)](https://crates.io/crates/terris)

A small, friendly Git worktree manager for everyday use.

## What it does
- Create new worktrees quickly
- List existing worktrees
- Remove worktrees safely
- Print a worktree path for `cd $(terris path <name>)`
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
terris list
terris create <name>
terris create <name> --branch <branch> --from <ref>
terris create <name> --path /tmp/my-wt
terris delete <name-or-path>
terris delete <name-or-path> --force
terris path <name-or-path>
```

## How create works
- Default branch is `<name>`.
- If the branch exists, it is used directly.
- If the branch does not exist, it is created (optionally from `--from`).
- Default path is `~/.terris-worktrees/<repo-name>/<branch>-<8-random-lowercase-letters>`.

## Examples
```bash
# Create a worktree (branch "feature-a") next to the repo root
terris create feature-a

# Create from main at a custom path
terris create feature-b --from main --path ../project-feature-b

# List worktrees
terris list

# Jump to a worktree
cd "$(terris path feature-a)"

# Delete a worktree
terris delete feature-a
```

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.

## Name
The project is named after the [Terris people](https://coppermind.net/wiki/Terris), responsible for preserving the knowledge of the civilization.

## License
MIT
