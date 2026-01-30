use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use rand::Rng;

#[derive(Parser)]
#[command(name = "terris", version, about = "Git worktree manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List worktrees for the current repository
    List,
    /// Create a new worktree
    Create {
        /// Name of the worktree (also default branch name)
        name: String,
        /// Optional path for the worktree
        #[arg(long)]
        path: Option<PathBuf>,
        /// Branch name to use/create (defaults to <name>)
        #[arg(long)]
        branch: Option<String>,
        /// Start point when creating a new branch
        #[arg(long)]
        from: Option<String>,
    },
    /// Remove a worktree
    Delete {
        /// Worktree name or path
        target: String,
        /// Force removal if the worktree has changes
        #[arg(long)]
        force: bool,
    },
    /// Print the path to a worktree
    Path {
        /// Worktree name or path
        target: String,
    },
}

#[derive(Debug, Default)]
struct Worktree {
    path: PathBuf,
    head: Option<String>,
    branch: Option<String>,
    detached: bool,
    locked: bool,
    prunable: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::List => cmd_list(),
        Commands::Create { name, path, branch, from } => cmd_create(&name, path, branch, from),
        Commands::Delete { target, force } => cmd_delete(&target, force),
        Commands::Path { target } => cmd_path(&target),
    }
}

fn cmd_list() -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    print_worktrees(&worktrees);
    Ok(())
}

fn cmd_create(name: &str, path: Option<PathBuf>, branch: Option<String>, from: Option<String>) -> Result<()> {
    let root = git_root()?;
    let cwd = std::env::current_dir().context("read current directory")?;
    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .to_string();
    let branch = branch.unwrap_or_else(|| name.to_string());
    let is_default_path = path.is_none();
    let target_path = match path {
        Some(p) => resolve_path(&cwd, p),
        None => default_worktree_path(&repo_name, &branch)?,
    };
    if is_default_path {
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("create worktree base directory '{}'", parent.display())
            })?;
        }
    }

    let branch_exists = git_branch_exists(&root, &branch)?;
    if branch_exists && from.is_some() {
        bail!("branch '{}' already exists; --from is only for new branches", branch);
    }

    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
    if !branch_exists {
        args.push("-b".into());
        args.push(branch.clone());
    }
    args.push(target_path.to_string_lossy().to_string());
    if branch_exists {
        args.push(branch.clone());
    } else if let Some(start) = from {
        args.push(start);
    }

    run_git(&args, &root).with_context(|| format!("create worktree '{}'", name))?;
    println!("{}", target_path.display());
    Ok(())
}

fn cmd_delete(target: &str, force: bool) -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    let wt = resolve_worktree(target, &worktrees)?;

    let mut args: Vec<String> = vec!["worktree".into(), "remove".into()];
    if force {
        args.push("--force".into());
    }
    args.push(wt.path.to_string_lossy().to_string());
    run_git(&args, &root).with_context(|| format!("remove worktree '{}'", target))?;
    Ok(())
}

fn cmd_path(target: &str) -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    let wt = resolve_worktree(target, &worktrees)?;
    println!("{}", wt.path.display());
    Ok(())
}

fn git_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("read current directory")?;
    let output = run_git(["rev-parse", "--show-toplevel"], &cwd)
        .context("not a git repository (or any parent)")?;
    Ok(PathBuf::from(output.trim()))
}

fn git_branch_exists(root: &Path, branch: &str) -> Result<bool> {
    let ref_name = format!("refs/heads/{}", branch);
    let status = Command::new("git")
        .arg("rev-parse")
        .arg("--verify")
        .arg("--quiet")
        .arg(ref_name)
        .current_dir(root)
        .status()
        .context("check branch existence")?;
    Ok(status.success())
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

fn print_worktrees(worktrees: &[Worktree]) {
    let mut rows: Vec<(String, String, String, String)> = Vec::new();
    for wt in worktrees {
        let name = worktree_name(wt);
        let branch = worktree_branch_short(wt).unwrap_or("-").to_string();
        let flags = worktree_flags(wt);
        let path = wt.path.to_string_lossy().to_string();
        rows.push((name, branch, path, flags));
    }

    let name_width = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max(4);
    let branch_width = rows.iter().map(|r| r.1.len()).max().unwrap_or(6).max(6);

    println!("{:name_width$} {:branch_width$} {} {}", "NAME", "BRANCH", "PATH", "FLAGS",
        name_width = name_width, branch_width = branch_width);
    for (name, branch, path, flags) in rows {
        println!("{:name_width$} {:branch_width$} {} {}", name, branch, path, flags,
            name_width = name_width, branch_width = branch_width);
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
    wt.branch.as_deref().map(|b| b.strip_prefix("refs/heads/").unwrap_or(b))
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

fn resolve_worktree<'a>(target: &str, worktrees: &'a [Worktree]) -> Result<&'a Worktree> {
    let cwd = std::env::current_dir().context("read current directory")?;
    let mut matches = match_by_path(target, worktrees, &cwd);
    if matches.is_empty() {
        matches = match_by_basename(target, worktrees);
    }
    if matches.is_empty() {
        matches = match_by_branch(target, worktrees);
    }
    if matches.is_empty() {
        bail!("no worktree matches '{}'", target);
    }
    if matches.len() > 1 {
        let names: Vec<String> = matches.iter().map(|w| w.path.display().to_string()).collect();
        bail!("'{}' is ambiguous: {}", target, names.join(", "));
    }
    Ok(matches[0])
}

fn match_by_path<'a>(target: &str, worktrees: &'a [Worktree], cwd: &Path) -> Vec<&'a Worktree> {
    let path_like = Path::new(target).is_absolute() || target.contains(std::path::MAIN_SEPARATOR);
    if !path_like {
        return Vec::new();
    }
    let target_path = resolve_path(cwd, PathBuf::from(target));
    let target_norm = normalize_path(&target_path);
    worktrees
        .iter()
        .filter(|w| normalize_path(&w.path) == target_norm)
        .collect()
}

fn match_by_basename<'a>(target: &str, worktrees: &'a [Worktree]) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|w| w.path.file_name().and_then(OsStr::to_str) == Some(target))
        .collect()
}

fn match_by_branch<'a>(target: &str, worktrees: &'a [Worktree]) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|w| worktree_branch_short(w) == Some(target))
        .collect()
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_path(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn default_worktree_path(repo_name: &str, branch: &str) -> Result<PathBuf> {
    let suffix = random_suffix(8);
    let base = registry_base_dir()?;
    Ok(base.join(repo_name).join(format!("{}-{}", branch, suffix)))
}

fn registry_base_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".terris-worktrees"))
}

fn random_suffix(len: usize) -> String {
    let mut rng = rand::thread_rng();
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let c = rng.gen_range(b'a'..=b'z') as char;
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        prior: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let prior = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, prior }
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
    fn resolve_worktree_matches_and_errors() {
        let temp_root = std::env::temp_dir().join("terris-tests-resolve");
        let _ = std::fs::create_dir_all(&temp_root);
        let wt1_path = temp_root.join("one");
        let wt2_path = temp_root.join("two");
        let _ = std::fs::create_dir_all(&wt1_path);
        let _ = std::fs::create_dir_all(&wt2_path);

        let worktrees = vec![
            wt(wt1_path.to_string_lossy().as_ref(), Some("refs/heads/alpha")),
            wt(wt2_path.to_string_lossy().as_ref(), Some("refs/heads/alpha")),
        ];

        let by_path = resolve_worktree(wt1_path.to_string_lossy().as_ref(), &worktrees).unwrap();
        assert_eq!(by_path.path, wt1_path);

        let err = resolve_worktree("alpha", &worktrees).unwrap_err();
        assert!(format!("{err}").contains("ambiguous"));

        let err = resolve_worktree("missing", &worktrees).unwrap_err();
        assert!(format!("{err}").contains("no worktree matches"));
    }

    #[test]
    fn default_worktree_path_uses_home_registry_and_suffix() {
        let temp_home = std::env::temp_dir().join("terris-tests-home");
        let _ = std::fs::create_dir_all(&temp_home);
        let _guard = EnvGuard::set("HOME", &temp_home);

        let path = default_worktree_path("repo", "branch").unwrap();
        let base = temp_home.join(".terris-worktrees").join("repo");
        assert!(path.starts_with(&base));

        let file_name = path.file_name().and_then(OsStr::to_str).unwrap();
        let suffix = file_name.strip_prefix("branch-").unwrap();
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn match_by_basename_and_branch() {
        let worktrees = vec![
            wt("/repo/alpha", Some("refs/heads/main")),
            wt("/repo/beta", Some("refs/heads/feature")),
        ];
        let by_base = match_by_basename("beta", &worktrees);
        assert_eq!(by_base.len(), 1);
        assert_eq!(by_base[0].path, PathBuf::from("/repo/beta"));

        let by_branch = match_by_branch("main", &worktrees);
        assert_eq!(by_branch.len(), 1);
        assert_eq!(by_branch[0].path, PathBuf::from("/repo/alpha"));
    }
}
