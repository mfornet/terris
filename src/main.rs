use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, ValueEnum};
use rand::Rng;

#[derive(Parser)]
#[command(name = "terris", version, about = "Git worktree manager")]
struct Cli {
    /// Print shell completion script (bash or zsh)
    #[arg(long, value_enum, conflicts_with_all = ["list", "delete", "branch"])]
    completions: Option<CompletionShell>,
    /// List worktrees for the current repository
    #[arg(long, conflicts_with_all = ["delete", "branch"])]
    list: bool,
    /// Remove a worktree by branch name
    #[arg(long, value_name = "branch", conflicts_with_all = ["list", "branch"])]
    delete: Option<String>,
    /// Branch name to open (create if missing)
    #[arg(value_name = "branch", conflicts_with_all = ["list", "delete"])]
    branch: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
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
    if let Some(shell) = cli.completions {
        print_completions(shell);
        return Ok(());
    }
    if cli.list {
        return cmd_list();
    }
    if let Some(branch) = cli.delete {
        return cmd_delete_branch(&branch);
    }
    if let Some(branch) = cli.branch {
        return cmd_ensure_branch(&branch);
    }
    Cli::command().print_help().context("print help")?;
    println!();
    Ok(())
}

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
    COMPREPLY=($(compgen -W "--list --delete" -- "$cur"))
    return 0
  fi

  if [[ $COMP_CWORD -eq 1 || "$prev" == "--delete" ]]; then
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
  '--list[List worktrees]' \
  '--delete[Remove a worktree by branch name]:branch:->branches' \
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

complete -c terris -l list -d 'List worktrees'
complete -c terris -l delete -d 'Remove a worktree by branch name' -a "(__terris_branches)"
complete -c terris -f -a "(__terris_branches)"
"#
            );
        }
    }
}

fn cmd_list() -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    print_worktrees(&worktrees);
    Ok(())
}

fn cmd_ensure_branch(branch: &str) -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    if let Some(wt) = find_worktree_by_branch(branch, &worktrees)? {
        println!("{}", wt.path.display());
        return Ok(());
    }

    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .to_string();
    let target_path = default_worktree_path(&repo_name, branch)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create worktree base directory '{}'", parent.display()))?;
    }

    let branch_exists = git_branch_exists(&root, branch)?;

    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
    if !branch_exists {
        args.push("-b".into());
        args.push(branch.to_string());
    }
    args.push(target_path.to_string_lossy().to_string());
    if branch_exists {
        args.push(branch.to_string());
    }

    run_git(&args, &root).with_context(|| format!("create worktree '{}'", branch))?;
    println!("{}", target_path.display());
    Ok(())
}

fn cmd_delete_branch(branch: &str) -> Result<()> {
    let root = git_root()?;
    let worktrees = list_worktrees(&root)?;
    let wt = find_worktree_by_branch(branch, &worktrees)?
        .with_context(|| format!("no worktree matches branch '{}'", branch))?;

    let mut args: Vec<String> = vec!["worktree".into(), "remove".into()];
    args.push(wt.path.to_string_lossy().to_string());
    run_git(&args, &root).with_context(|| format!("remove worktree '{}'", branch))?;
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

    println!(
        "{:name_width$} {:branch_width$} PATH FLAGS",
        "NAME",
        "BRANCH",
        name_width = name_width,
        branch_width = branch_width
    );
    for (name, branch, path, flags) in rows {
        println!(
            "{:name_width$} {:branch_width$} {} {}",
            name,
            branch,
            path,
            flags,
            name_width = name_width,
            branch_width = branch_width
        );
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

        let path = default_worktree_path("repo", "branch").unwrap();
        let base = temp_home.join(".terris-worktrees").join("repo");
        assert!(path.starts_with(&base));

        let file_name = path.file_name().and_then(OsStr::to_str).unwrap();
        let suffix = file_name.strip_prefix("branch-").unwrap();
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_lowercase()));
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
}
