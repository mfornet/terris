Handoff notes for future work

Project intent
- `terris` is a git worktree manager CLI built in Rust.
- Operates from any directory inside a git repo; resolves the repo root via `git`.
- Uses the `git worktree` subcommands directly; no custom git plumbing.

Command summary
- `terris <branch>`
- `terris --list`
- `terris --delete <branch>`

Key implementation details
- Parsing uses `git worktree list --porcelain` to avoid brittle parsing.
- Branch detection: `refs/heads/<name>` is checked via `git rev-parse --verify --quiet`.
- Ensure behavior: if branch exists, `git worktree add <path> <branch>`;
  otherwise `git worktree add -b <branch> <path>` from current HEAD.
- Default path is computed from registry: `~/.terris-worktrees/<repo-name>/<branch>-<8-random-lowercase-letters>`.
- Worktree matching is by branch short-name only.
- Errors are surfaced with `anyhow` and clear messages.

Build/run
- Build: `cargo build`
- Run: `cargo run -- <subcommand>`
- No tests yet.
- After every code update, run `cargo fmt` and `cargo clippy`.
- After changes, run relevant tests.

Potential future improvements
- `terris prune` wrapper.
- JSON output for list.
- Configurable default worktree base directory.
- Completion scripts for bash/zsh/fish.
