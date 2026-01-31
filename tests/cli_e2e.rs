use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

fn run_git(args: &[&str], cwd: &std::path::Path) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("run git command");
    assert!(status.success(), "git {:?} failed", args);
}

fn run_git_with_env(args: &[&str], cwd: &std::path::Path, envs: &[(&str, &str)]) {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let status = cmd.status().expect("run git command");
    assert!(status.success(), "git {:?} failed", args);
}

#[test]
fn worktree_create_stdout_is_single_line() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let repo_dir = temp_dir.path().join("repo");
    std::fs::create_dir_all(&repo_dir).expect("create repo dir");

    run_git(&["init"], &repo_dir);

    std::fs::write(repo_dir.join("README.md"), "test\n").expect("write file");
    run_git_with_env(
        &["-c", "user.name=Test", "-c", "user.email=test@example.com", "add", "."],
        &repo_dir,
        &[],
    );
    run_git_with_env(
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ],
        &repo_dir,
        &[],
    );

    run_git(&["branch", "feature"], &repo_dir);

    let home_dir = temp_dir.path().join("home");
    std::fs::create_dir_all(&home_dir).expect("create home dir");

    let bin = cargo_bin!("terris");
    let output = Command::new(bin)
        .arg("feature")
        .current_dir(&repo_dir)
        .env("HOME", &home_dir)
        .output()
        .expect("run terris");

    assert!(output.status.success(), "terris failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout should be a single line: {stdout:?}");
}
