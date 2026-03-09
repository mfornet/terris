use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use rand::Rng;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "terris", version, about = "Git worktree manager")]
struct Cli {
    /// Print shell completion script (bash or zsh)
    #[arg(long, value_enum, conflicts_with_all = ["all", "rm", "branch"])]
    completions: Option<CompletionShell>,
    /// List all worktrees, including those without branches
    #[arg(long, conflicts_with_all = ["rm", "branch"])]
    all: bool,
    /// Remove a worktree by branch name
    #[arg(long = "rm", value_name = "branch", conflicts_with_all = ["branch"])]
    rm: Option<String>,
    /// Branch name to open (create if missing)
    #[arg(value_name = "branch", conflicts_with_all = ["all", "rm"])]
    branch: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Loaded by layering:
/// 1. `~/.terris/terris.toml` (user-global defaults)
/// 2. `<git-root>/.terris.toml` (project-local overrides)
///
/// If no file is found, all fields use defaults.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Config {
    worktrees: WorktreesConfig,
    behavior: BehaviorConfig,
    display: DisplayConfig,
}

/// `[worktrees]` — controls where and how worktree directories are created.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct WorktreesConfig {
    /// Base directory for new worktrees. Supports `~`. Default: `~/.terris-worktrees`.
    base_dir: Option<String>,
    /// Append a random suffix to new worktree directory names. Default: `true`.
    use_random_suffix: Option<bool>,
    /// Length of the random suffix (only relevant when `use_random_suffix = true`). Default: `8`.
    suffix_length: Option<usize>,
}

impl WorktreesConfig {
    fn use_random_suffix(&self) -> bool {
        self.use_random_suffix.unwrap_or(true)
    }
    fn suffix_length(&self) -> usize {
        self.suffix_length.unwrap_or(8)
    }
    fn validated_suffix_length(&self) -> Result<usize> {
        let len = self.suffix_length();
        if !(1..=64).contains(&len) {
            bail!(
                "invalid configuration: worktrees.suffix_length must be between 1 and 64 (got {})",
                len
            );
        }
        Ok(len)
    }
}

/// `[behavior]` — controls what happens when the requested branch is not found locally.
///
/// `on_missing_branch` is a comma-separated list of actions tried in order:
/// - `error`  — fail immediately (the default when the field is absent)
/// - `fetch`  — run `git fetch origin <branch>` and use the remote tracking ref if found
/// - `create` — create a fresh local branch from HEAD if nothing else succeeded
///
/// `error` is terminal and cannot be combined with other actions.
///
/// Examples:
/// ```toml
/// on_missing_branch = "fetch"           # fetch from remote; error if truly not found
/// on_missing_branch = "fetch, create"   # fetch first, create if still not found
/// on_missing_branch = "create"          # always create a new local branch
/// on_missing_branch = "error"           # current default behaviour
/// ```
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct BehaviorConfig {
    on_missing_branch: MissingBranchStrategy,
    /// Run `git worktree prune` silently before listing worktrees. Default: `false`.
    auto_prune: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MissingBranchAction {
    Error,
    Fetch,
    Create,
}

/// Parsed representation of `on_missing_branch` preserving action order.
#[derive(Debug, Clone)]
struct MissingBranchStrategy {
    actions: Vec<MissingBranchAction>,
}

impl Default for MissingBranchStrategy {
    fn default() -> Self {
        Self {
            actions: vec![MissingBranchAction::Error],
        }
    }
}

impl MissingBranchStrategy {
    fn actions(&self) -> &[MissingBranchAction] {
        &self.actions
    }
}

impl<'de> Deserialize<'de> for MissingBranchStrategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use std::collections::HashSet;

        let s = String::deserialize(deserializer)?;
        let mut actions = Vec::new();
        let mut seen = HashSet::new();
        for part in s.split(',') {
            let token = part.trim();
            if token.is_empty() {
                return Err(serde::de::Error::custom(
                    "on_missing_branch cannot contain empty actions",
                ));
            }
            let action = match token {
                "error" => MissingBranchAction::Error,
                "fetch" => MissingBranchAction::Fetch,
                "create" => MissingBranchAction::Create,
                other => {
                    return Err(serde::de::Error::custom(format!(
                        "unknown on_missing_branch value '{other}'; valid values: error, fetch, create"
                    )));
                }
            };
            if !seen.insert(action) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate on_missing_branch action '{token}'"
                )));
            }
            actions.push(action);
        }
        if actions.is_empty() {
            return Err(serde::de::Error::custom(
                "on_missing_branch must contain at least one action",
            ));
        }
        if actions.contains(&MissingBranchAction::Error) && actions.len() > 1 {
            return Err(serde::de::Error::custom(
                "on_missing_branch action 'error' cannot be combined with other actions",
            ));
        }
        Ok(Self { actions })
    }
}

/// `[display]` — controls what information is printed.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DisplayConfig {
    /// Show the (short) HEAD commit hash as an extra column when listing worktrees. Default: `false`.
    show_head: bool,
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

fn load_config() -> Result<Config> {
    let project_root = git_root().ok();
    let mut merged = toml::Value::Table(toml::map::Map::new());
    for path in config_file_candidates(project_root.as_deref()) {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("read config '{}'", path.display()))?;
        let parsed: toml::Value = toml::from_str(&content)
            .with_context(|| format!("parse config '{}'", path.display()))?;
        merge_toml_values(&mut merged, parsed);
    }
    let config: Config = merged
        .try_into()
        .context("deserialize merged configuration")?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &Config) -> Result<()> {
    if config.worktrees.use_random_suffix() {
        let _ = config.worktrees.validated_suffix_length()?;
    }
    Ok(())
}

fn config_file_candidates(project_root: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    // 1. ~/.terris/terris.toml — user-global defaults.
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".terris").join("terris.toml"));
    }
    // 2. <git-root>/.terris.toml — project-local overrides.
    if let Some(root) = project_root {
        candidates.push(root.join(".terris.toml"));
    } else if let Ok(cwd) = std::env::current_dir() {
        // Best-effort fallback when not in a repository.
        candidates.push(cwd.join(".terris.toml"));
    }
    candidates
}

fn merge_toml_values(dst: &mut toml::Value, src: toml::Value) {
    match (dst, src) {
        (toml::Value::Table(dst_table), toml::Value::Table(src_table)) => {
            for (k, src_value) in src_table {
                if let Some(dst_value) = dst_table.get_mut(&k) {
                    merge_toml_values(dst_value, src_value);
                } else {
                    dst_table.insert(k, src_value);
                }
            }
        }
        (dst_slot, src_value) => {
            *dst_slot = src_value;
        }
    }
}

/// Expand a leading `~/` or lone `~` using the `HOME` env var.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path)
}

// ---------------------------------------------------------------------------
// Worktree struct
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Worktree {
    path: PathBuf,
    head: Option<String>,
    branch: Option<String>,
    detached: bool,
    locked: bool,
    prunable: Option<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        print_completions(shell);
        return Ok(());
    }
    let config = load_config()?;
    if let Some(branch) = cli.rm {
        return cmd_delete_branch(&branch);
    }
    if let Some(branch) = cli.branch {
        return cmd_ensure_branch(&branch, &config);
    }
    cmd_list(cli.all, &config)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_list(show_all: bool, config: &Config) -> Result<()> {
    let root = git_root()?;
    if config.behavior.auto_prune {
        // Best-effort; ignore failures (e.g. no stale entries to prune).
        let _ = run_git_silence_stdout(["worktree", "prune"], &root);
    }
    let worktrees = list_worktrees(&root)?;
    if show_all {
        print_worktrees(&worktrees, config.display.show_head);
        return Ok(());
    }
    let (with_branch, without_branch): (Vec<Worktree>, Vec<Worktree>) = worktrees
        .into_iter()
        .partition(|wt| worktree_branch_short(wt).is_some());
    print_worktrees(&with_branch, config.display.show_head);
    if !without_branch.is_empty() {
        println!(
            "# {} worktree(s) without a branch not shown. Use --all to display.",
            without_branch.len()
        );
    }
    Ok(())
}

fn cmd_ensure_branch(branch: &str, config: &Config) -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;

    // Already has a worktree for this branch — just print its path.
    if let Some(wt) = find_worktree_by_branch(branch, &worktrees)? {
        println!("{}", wt.path.display());
        return Ok(());
    }

    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .to_string();
    let target_path = default_worktree_path(&repo_name, branch, config)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create worktree base directory '{}'", parent.display()))?;
    }

    // 1. Local branch exists → create worktree directly.
    if git_branch_exists_local(&root, branch)? {
        create_worktree_local(&root, branch, &target_path)?;
        println!("{}", target_path.display());
        return Ok(());
    }

    let strategy = &config.behavior.on_missing_branch;
    let mut fetch_attempted = false;
    for action in strategy.actions() {
        match action {
            MissingBranchAction::Error => {
                bail!(
                    "branch '{}' does not exist locally. Hint: add `on_missing_branch = \"fetch, create\"` to your terris config",
                    branch
                );
            }
            MissingBranchAction::Fetch => {
                fetch_attempted = true;
                if git_fetch_branch(&root, branch)? {
                    create_worktree_from_remote(&root, branch, &target_path)?;
                    println!("{}", target_path.display());
                    return Ok(());
                }
            }
            MissingBranchAction::Create => {
                create_worktree_new_branch(&root, branch, &target_path)?;
                println!("{}", target_path.display());
                return Ok(());
            }
        }
    }

    if fetch_attempted {
        bail!("branch '{}' does not exist locally and was not found on the remote", branch);
    }
    bail!(
        "branch '{}' does not exist locally. Hint: add `on_missing_branch = \"fetch, create\"` to your terris config",
        branch
    );
}

fn cmd_delete_branch(branch: &str) -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    let wt = find_worktree_by_branch(branch, &worktrees)?
        .with_context(|| format!("no worktree matches branch '{}'", branch))?;
    let path_str = wt.path.to_string_lossy().to_string();
    run_git_silence_stdout(["worktree", "remove", &path_str], &root)
        .with_context(|| format!("remove worktree '{}'", branch))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Branch / worktree helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `refs/heads/<branch>` exists in the repo.
fn git_branch_exists_local(root: &Path, branch: &str) -> Result<bool> {
    let ref_name = format!("refs/heads/{}", branch);
    let status = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &ref_name])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("check local branch existence")?;
    Ok(status.success())
}

/// Runs `git fetch origin <branch>`. Returns `true` if the fetch succeeded and
/// the remote tracking ref `refs/remotes/origin/<branch>` now exists.
fn git_fetch_branch(root: &Path, branch: &str) -> Result<bool> {
    let fetch = Command::new("git")
        .args(["fetch", "origin", branch])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("run git fetch")?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        if is_missing_remote_ref_error(&stderr) {
            return Ok(false);
        }
        bail!("git fetch origin {} failed: {}", branch, stderr.trim());
    }
    // Confirm the remote tracking ref actually landed.
    let ref_name = format!("refs/remotes/origin/{}", branch);
    let check = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &ref_name])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("check remote tracking ref")?;
    Ok(check.success())
}

fn is_missing_remote_ref_error(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("couldn't find remote ref") || lower.contains("could not find remote ref")
}

/// `git worktree add --quiet <path> <branch>`  — branch must already exist locally.
fn create_worktree_local(root: &Path, branch: &str, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    run_git_silence_stdout(["worktree", "add", "--quiet", &path_str, branch], root)
        .with_context(|| format!("create worktree for existing branch '{}'", branch))
}

/// `git worktree add --quiet --track -b <branch> <path> origin/<branch>`
/// Creates a local branch tracking the remote, then checks it out into a new worktree.
fn create_worktree_from_remote(root: &Path, branch: &str, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let remote_ref = format!("origin/{}", branch);
    run_git_silence_stdout(
        [
            "worktree", "add", "--quiet", "--track", "-b", branch, &path_str, &remote_ref,
        ],
        root,
    )
    .with_context(|| format!("create worktree for remote branch '{}'", branch))
}

/// `git worktree add --quiet -b <branch> <path>`  — creates a fresh local branch from HEAD.
fn create_worktree_new_branch(root: &Path, branch: &str, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    run_git_silence_stdout(
        ["worktree", "add", "--quiet", "-b", branch, &path_str],
        root,
    )
    .with_context(|| format!("create new worktree branch '{}'", branch))
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn default_worktree_path(repo_name: &str, branch: &str, config: &Config) -> Result<PathBuf> {
    let base = registry_base_dir(config)?;
    if config.worktrees.use_random_suffix() {
        let suffix = random_suffix(config.worktrees.validated_suffix_length()?);
        Ok(base.join(repo_name).join(format!("{}-{}", branch, suffix)))
    } else {
        Ok(base.join(repo_name).join(branch))
    }
}

fn registry_base_dir(config: &Config) -> Result<PathBuf> {
    if let Some(base_dir) = &config.worktrees.base_dir {
        return Ok(expand_tilde(base_dir));
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".terris-worktrees"))
}

fn random_suffix(len: usize) -> String {
    let mut rng = rand::rng();
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let c = rng.random_range(b'a'..=b'z') as char;
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Shell completions
// ---------------------------------------------------------------------------

fn print_completions(shell: CompletionShell) {
    match shell {
        CompletionShell::Bash => {
            println!(
                r#"_terris_branches() {{
  git for-each-ref --format='%(refname:short)' refs/heads 2>/dev/null
}}

_terris_complete() {{
  local cur prev
  cur="${{COMP_WORDS[COMP_CWORD]}}"
  prev="${{COMP_WORDS[COMP_CWORD-1]}}"

  if [[ "$cur" == -* ]]; then
    COMPREPLY=($(compgen -W "--all --rm" -- "$cur"))
    return 0
  fi

  if [[ $COMP_CWORD -eq 1 || "$prev" == "--rm" ]]; then
    COMPREPLY=($(compgen -W "$(_terris_branches)" -- "$cur"))
    return 0
  fi

  COMPREPLY=()
}}

complete -F _terris_complete terris
"#
            );
        }
        CompletionShell::Zsh => {
            println!(
                r#"#compdef terris

_terris_branches() {{
  git for-each-ref --format='%(refname:short)' refs/heads 2>/dev/null
}}

_arguments -s \
  '--all[List all worktrees, including those without branches]' \
  '--rm[Remove a worktree by branch name]:branch:->branches' \
  '1:branch:->branches' \
  '*: :->args'

case $state in
  branches)
    _values 'branches' $(_terris_branches)
    ;;
esac
"#
            );
        }
        CompletionShell::Fish => {
            println!(
                r#"function __terris_branches
  command git for-each-ref --format='%(refname:short)' refs/heads 2>/dev/null
end

complete -c terris -l all -d 'List all worktrees, including those without branches'
complete -c terris -l rm -d 'Remove a worktree by branch name' -a "(__terris_branches)"
complete -c terris -f -a "(__terris_branches)"
"#
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

fn git_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("read current directory")?;
    let output = run_git(["rev-parse", "--show-toplevel"], &cwd)
        .context("not a git repository (or any parent)")?;
    Ok(PathBuf::from(output.trim()))
}

fn list_worktrees(root: &Path) -> Result<Vec<Worktree>> {
    let output = run_git(["worktree", "list", "--porcelain"], root)?;
    Ok(parse_worktrees(&output))
}

fn parse_worktrees(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            current = Some(Worktree {
                path: PathBuf::from(path.trim()),
                ..Worktree::default()
            });
            continue;
        }
        if let Some(wt) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                wt.head = Some(head.trim().to_string());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                wt.branch = Some(branch.trim().to_string());
            } else if line.trim() == "detached" {
                wt.detached = true;
            } else if line.trim() == "locked" {
                wt.locked = true;
            } else if let Some(prunable) = line.strip_prefix("prunable ") {
                wt.prunable = Some(prunable.trim().to_string());
            }
        }
    }
    if let Some(wt) = current.take() {
        worktrees.push(wt);
    }
    worktrees
}

fn run_git<I, S>(args: I, cwd: &Path) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().to_string())
        .collect();
    let output = Command::new("git")
        .args(&args_vec)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("run git {}", args_vec.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git_silence_stdout<I, S>(args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().to_string())
        .collect();
    let output = Command::new("git")
        .args(&args_vec)
        .current_dir(cwd)
        .stdout(Stdio::null())
        .output()
        .with_context(|| format!("run git {}", args_vec.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

fn print_worktrees(worktrees: &[Worktree], show_head: bool) {
    let mut rows: Vec<(String, String, String, String, String)> = Vec::new();
    for wt in worktrees {
        let name = worktree_name(wt);
        let branch = worktree_branch_short(wt).unwrap_or("-").to_string();
        let flags = worktree_flags(wt);
        let path = wt.path.to_string_lossy().to_string();
        // Show first 7 chars of the SHA (standard short-hash length).
        let head = wt
            .head
            .as_deref()
            .map(|h| h.get(..7).unwrap_or(h))
            .unwrap_or("-")
            .to_string();
        rows.push((name, branch, path, flags, head));
    }

    let name_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max(4);
    let branch_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(6).max(6);

    if show_head {
        println!(
            "{:name_w$} {:branch_w$} {:7} PATH FLAGS",
            "NAME", "BRANCH", "HEAD",
        );
        for (name, branch, path, flags, head) in &rows {
            println!(
                "{:name_w$} {:branch_w$} {:7} {} {}",
                name, branch, head, path, flags,
            );
        }
    } else {
        println!("{:name_w$} {:branch_w$} PATH FLAGS", "NAME", "BRANCH",);
        for (name, branch, path, flags, _) in &rows {
            println!("{:name_w$} {:branch_w$} {} {}", name, branch, path, flags,);
        }
    }
}

fn worktree_name(wt: &Worktree) -> String {
    if let Some(branch) = worktree_branch_short(wt) {
        return branch.to_string();
    }
    wt.path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("-")
        .to_string()
}

fn worktree_branch_short(wt: &Worktree) -> Option<&str> {
    wt.branch
        .as_deref()
        .map(|b| b.strip_prefix("refs/heads/").unwrap_or(b))
}

fn worktree_flags(wt: &Worktree) -> String {
    let mut flags = Vec::new();
    if wt.detached {
        flags.push("detached");
    }
    if wt.locked {
        flags.push("locked");
    }
    if wt.prunable.is_some() {
        flags.push("prunable");
    }
    if flags.is_empty() {
        "-".to_string()
    } else {
        flags.join(",")
    }
}

fn find_worktree_by_branch<'a>(
    branch: &str,
    worktrees: &'a [Worktree],
) -> Result<Option<&'a Worktree>> {
    let matches: Vec<&Worktree> = worktrees
        .iter()
        .filter(|w| worktree_branch_short(w) == Some(branch))
        .collect();
    if matches.is_empty() {
        return Ok(None);
    }
    if matches.len() > 1 {
        let names: Vec<String> = matches
            .iter()
            .map(|w| w.path.display().to_string())
            .collect();
        bail!("branch '{}' is ambiguous: {}", branch, names.join(", "));
    }
    Ok(Some(matches[0]))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex, MutexGuard};

    static ENV_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct EnvGuard {
        key: &'static str,
        prior: Option<std::ffi::OsString>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let lock = ENV_MUTEX.lock().expect("lock ENV mutex");
            let prior = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key,
                prior,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prior {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn wt(path: &str, branch: Option<&str>) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            branch: branch.map(|b| b.to_string()),
            ..Worktree::default()
        }
    }

    #[test]
    fn parse_worktrees_parses_porcelain() {
        let input = "\
worktree /repo
HEAD 111111
branch refs/heads/main

worktree /repo/feature
HEAD 222222
detached
locked
prunable stale
";
        let worktrees = parse_worktrees(input);
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, PathBuf::from("/repo"));
        assert_eq!(worktrees[0].head.as_deref(), Some("111111"));
        assert_eq!(worktrees[0].branch.as_deref(), Some("refs/heads/main"));
        assert!(!worktrees[0].detached);
        assert!(!worktrees[0].locked);
        assert!(worktrees[0].prunable.is_none());

        assert_eq!(worktrees[1].path, PathBuf::from("/repo/feature"));
        assert_eq!(worktrees[1].head.as_deref(), Some("222222"));
        assert!(worktrees[1].branch.is_none());
        assert!(worktrees[1].detached);
        assert!(worktrees[1].locked);
        assert_eq!(worktrees[1].prunable.as_deref(), Some("stale"));
    }

    #[test]
    fn worktree_display_helpers() {
        let mut wt = Worktree {
            path: PathBuf::from("/repo/feature"),
            branch: Some("refs/heads/feature".into()),
            detached: true,
            locked: true,
            prunable: Some("gone".into()),
            ..Worktree::default()
        };
        assert_eq!(worktree_branch_short(&wt), Some("feature"));
        assert_eq!(worktree_name(&wt), "feature");
        assert_eq!(worktree_flags(&wt), "detached,locked,prunable");

        wt.branch = None;
        assert_eq!(worktree_name(&wt), "feature");
        wt.detached = false;
        wt.locked = false;
        wt.prunable = None;
        assert_eq!(worktree_flags(&wt), "-");
    }

    #[test]
    fn find_worktree_by_branch_matches_and_errors() {
        let worktrees = vec![
            wt("/repo/one", Some("refs/heads/alpha")),
            wt("/repo/two", Some("refs/heads/alpha")),
        ];

        let err = find_worktree_by_branch("alpha", &worktrees).unwrap_err();
        assert!(format!("{err}").contains("ambiguous"));

        let missing = find_worktree_by_branch("missing", &worktrees).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn default_worktree_path_uses_home_registry_and_suffix() {
        let temp_home = std::env::temp_dir().join("terris-tests-home");
        let _ = std::fs::create_dir_all(&temp_home);
        let _guard = EnvGuard::set("HOME", &temp_home);

        let config = Config::default(); // use_random_suffix=true, suffix_length=8
        let path = default_worktree_path("repo", "branch", &config).unwrap();
        let base = temp_home.join(".terris-worktrees").join("repo");
        assert!(path.starts_with(&base));

        let file_name = path.file_name().and_then(OsStr::to_str).unwrap();
        let suffix = file_name.strip_prefix("branch-").unwrap();
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn default_worktree_path_no_suffix() {
        let temp_home = std::env::temp_dir().join("terris-tests-home-nosuffix");
        let _ = std::fs::create_dir_all(&temp_home);
        let _guard = EnvGuard::set("HOME", &temp_home);

        let config = Config {
            worktrees: WorktreesConfig {
                use_random_suffix: Some(false),
                ..WorktreesConfig::default()
            },
            ..Config::default()
        };
        let path = default_worktree_path("repo", "my-branch", &config).unwrap();
        assert_eq!(
            path,
            temp_home
                .join(".terris-worktrees")
                .join("repo")
                .join("my-branch")
        );
    }

    #[test]
    fn find_worktree_by_branch_matches() {
        let worktrees = vec![
            wt("/repo/alpha", Some("refs/heads/main")),
            wt("/repo/beta", Some("refs/heads/feature")),
        ];

        let by_branch = find_worktree_by_branch("main", &worktrees).unwrap();
        assert_eq!(by_branch.unwrap().path, PathBuf::from("/repo/alpha"));
    }

    #[test]
    fn config_base_dir_tilde_expansion() {
        let temp_home = std::env::temp_dir().join("terris-tests-tilde");
        let _ = std::fs::create_dir_all(&temp_home);
        let _guard = EnvGuard::set("HOME", &temp_home);

        let config = Config {
            worktrees: WorktreesConfig {
                base_dir: Some("~/my-worktrees".to_string()),
                ..WorktreesConfig::default()
            },
            ..Config::default()
        };
        let base = registry_base_dir(&config).unwrap();
        assert_eq!(base, temp_home.join("my-worktrees"));
    }

    #[test]
    fn missing_branch_strategy_parse_preserves_order() {
        fn parse(s: &str) -> Vec<MissingBranchAction> {
            toml::from_str::<BehaviorConfig>(&format!("on_missing_branch = \"{s}\""))
                .unwrap()
                .on_missing_branch
                .actions
        }

        assert_eq!(parse("error"), vec![MissingBranchAction::Error]);
        assert_eq!(parse("fetch"), vec![MissingBranchAction::Fetch]);
        assert_eq!(parse("create"), vec![MissingBranchAction::Create]);
        assert_eq!(
            parse("fetch, create"),
            vec![MissingBranchAction::Fetch, MissingBranchAction::Create]
        );
        assert_eq!(
            parse("create, fetch"),
            vec![MissingBranchAction::Create, MissingBranchAction::Fetch]
        );
    }

    #[test]
    fn missing_branch_strategy_rejects_invalid_combinations() {
        let with_error =
            toml::from_str::<BehaviorConfig>("on_missing_branch = \"error, fetch\"");
        assert!(with_error.is_err());

        let with_duplicate =
            toml::from_str::<BehaviorConfig>("on_missing_branch = \"fetch, fetch\"");
        assert!(with_duplicate.is_err());

        let with_empty =
            toml::from_str::<BehaviorConfig>("on_missing_branch = \"fetch, \"");
        assert!(with_empty.is_err());

        let result =
            toml::from_str::<BehaviorConfig>("on_missing_branch = \"teleport\"");
        assert!(result.is_err());
    }

    #[test]
    fn merge_toml_values_layers_global_and_local() {
        let mut merged: toml::Value =
            toml::from_str("[display]\nshow_head = true\n").unwrap();
        let local: toml::Value = toml::from_str(
            "[worktrees]\nuse_random_suffix = false\n[display]\nshow_head = false\n",
        )
        .unwrap();

        merge_toml_values(&mut merged, local);
        let config: Config = merged.try_into().unwrap();
        assert_eq!(config.worktrees.use_random_suffix, Some(false));
        assert!(!config.display.show_head);
    }

    #[test]
    fn config_file_candidates_prefers_git_root_for_local_path() {
        let home = PathBuf::from("/tmp/fake-home");
        let _guard = EnvGuard::set("HOME", &home);
        let root = PathBuf::from("/tmp/repo-root");
        let candidates = config_file_candidates(Some(&root));
        assert_eq!(
            candidates,
            vec![
                home.join(".terris").join("terris.toml"),
                root.join(".terris.toml")
            ]
        );
    }

    #[test]
    fn default_worktree_path_rejects_invalid_suffix_length() {
        let config = Config {
            worktrees: WorktreesConfig {
                suffix_length: Some(0),
                ..WorktreesConfig::default()
            },
            ..Config::default()
        };
        let err =
            default_worktree_path("repo", "branch", &config).unwrap_err().to_string();
        assert!(err.contains("worktrees.suffix_length"));
    }
}
