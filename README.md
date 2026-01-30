# woody

A small, friendly Git worktree manager for everyday use.

## What it does
- Create new worktrees quickly
- List existing worktrees
- Remove worktrees safely
- Print a worktree path for `cd $(wood path <name>)`

## Install
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
woody list
woody create <name>
woody create <name> --branch <branch> --from <ref>
woody create <name> --path /tmp/my-wt
woody delete <name-or-path>
woody delete <name-or-path> --force
woody path <name-or-path>
```

## How create works
- Default branch is `<name>`.
- If the branch exists, it is used directly.
- If the branch does not exist, it is created (optionally from `--from`).
- Default path is `~/.wood-worktrees/<repo-name>/<branch>-<8-random-lowercase-letters>`.

## Examples
```bash
# Create a worktree (branch "feature-a") next to the repo root
woody create feature-a

# Create from main at a custom path
woody create feature-b --from main --path ../project-feature-b

# List worktrees
woody list

# Jump to a worktree
cd "$(woody path feature-a)"

# Delete a worktree
woody delete feature-a
```

## Notes
- Works from any directory inside a git repo.
- The tool shells out to `git`, so `git` must be installed and available in `PATH`.
