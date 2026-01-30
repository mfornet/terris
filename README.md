# wood

A small, friendly Git worktree manager for everyday use.

## What it does
- Create new worktrees quickly
- List existing worktrees
- Remove worktrees safely
- Print a worktree path for `cd $(wood path <name>)`

## Install
From source:
```bash
cargo build --release
```

## Usage
```bash
wood list
wood create <name>
wood create <name> --branch <branch> --from <ref>
wood create <name> --path /tmp/my-wt
wood delete <name-or-path>
wood delete <name-or-path> --force
wood path <name-or-path>
```

## How create works
- Default branch is `<name>`.
- If the branch exists, it is used directly.
- If the branch does not exist, it is created (optionally from `--from`).
- Default path is `~/.wood-worktrees/<repo-name>/<branch>-<8-random-lowercase-letters>`.

## Examples
```bash
# Create a worktree (branch "feature-a") next to the repo root
wood create feature-a

# Create from main at a custom path
wood create feature-b --from main --path ../project-feature-b

# List worktrees
wood list

# Jump to a worktree
cd "$(wood path feature-a)"

# Delete a worktree
wood delete feature-a
```

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.
