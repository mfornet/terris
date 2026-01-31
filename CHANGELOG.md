# Changelog

## 1.0.4 - 2026-01-31
- Silence git worktree/branch helper output so `terris <branch>` prints only the worktree path.
- Add end-to-end CLI test covering worktree creation stdout (with test-only deps).
- List worktrees by default and use `--rm` to remove worktrees.

## 1.0.3 - 2026-01-30
- Require branches to exist; `terris <branch>` errors if missing.

## 1.0.2 - 2026-01-30
- Add shell completions for bash, zsh, and fish.
