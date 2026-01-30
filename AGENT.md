Handoff notes for future work

Project intent
- `woody` is a git worktree manager CLI built in Rust.
- Operates from any directory inside a git repo; resolves the repo root via `git`.
- Uses the `git worktree` subcommands directly; no custom git plumbing.

Command summary
- `woody list`
- `woody create <name> [--path <path>] [--branch <branch>] [--from <ref>]`
- `woody delete <name-or-path> [--force]`
- `woody path <name-or-path>`

Key implementation details
- Parsing uses `git worktree list --porcelain` to avoid brittle parsing.
- Branch detection: `refs/heads/<name>` is checked via `git rev-parse --verify --quiet`.
- Create behavior: if branch exists, `git worktree add <path> <branch>`;
  otherwise `git worktree add -b <branch> <path> [<from>]`.
- Default path is computed from registry: `~/.wood-worktrees/<repo-name>/<branch>-<8-random-lowercase-letters>`.
- Worktree matching resolution order: exact path, basename, branch short-name.
- Errors are surfaced with `anyhow` and clear messages.

Build/run
- Build: `cargo build`
- Run: `cargo run -- <subcommand>`
- No tests yet.

Potential future improvements
- `wood prune` wrapper.
- JSON output for list.
- Configurable default worktree base directory.
- Completion scripts for bash/zsh/fish.
