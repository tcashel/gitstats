use gitstats::analysis::analyze_repo_async;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_git_command(repo_path: &Path, args: &[&str]) -> String {
    Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default()
}

fn get_git_commit_count(repo_path: &Path) -> usize {
    let output = run_git_command(repo_path, &["rev-list", "--count", "HEAD"]);
    output.trim().parse().unwrap_or(0)
}

fn get_git_line_stats(repo_path: &Path) -> (usize, usize) {
    let output = run_git_command(repo_path, &["log", "--numstat"]);
    let mut added = 0;
    let mut deleted = 0;

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            if let (Ok(a), Ok(d)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                added += a;
                deleted += d;
            }
        }
    }
    (added, deleted)
}

#[tokio::test]
async fn test_commit_count_accuracy() {
    // Set up test repo (using the same setup as benchmarks)
    let temp_dir = TempDir::new().unwrap();
    Command::new("git")
        .current_dir(&temp_dir)
        .args(&["clone", "https://github.com/BurntSushi/ripgrep.git", "."])
        .status()
        .expect("Failed to clone ripgrep repository");

    let repo_path = temp_dir.path();

    // Get git's count
    let git_count = get_git_commit_count(repo_path);

    // Get our count
    let result = analyze_repo_async(
        repo_path.to_str().unwrap().to_string(),
        "main".to_string(),
        "All".to_string(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        git_count, result.commit_count,
        "Commit counts don't match! Git: {}, Ours: {}",
        git_count, result.commit_count
    );
}

#[tokio::test]
#[ignore]
async fn test_line_stats_accuracy() {
    // Set up test repo
    let temp_dir = TempDir::new().unwrap();
    Command::new("git")
        .current_dir(&temp_dir)
        .args(&["clone", "https://github.com/BurntSushi/ripgrep.git", "."])
        .status()
        .expect("Failed to clone ripgrep repository");

    let repo_path = temp_dir.path();

    // Get git's stats
    let (git_added, git_deleted) = get_git_line_stats(repo_path);

    // Get our stats
    let result = analyze_repo_async(
        repo_path.to_str().unwrap().to_string(),
        "main".to_string(),
        "All".to_string(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        git_added, result.total_lines_added,
        "Lines added don't match! Git: {}, Ours: {}",
        git_added, result.total_lines_added
    );

    assert_eq!(
        git_deleted, result.total_lines_deleted,
        "Lines deleted don't match! Git: {}, Ours: {}",
        git_deleted, result.total_lines_deleted
    );
}
