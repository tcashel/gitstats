use chrono::{DateTime, Utc};
use git2::{Error, Repository};
use std::collections::HashMap;
use tokio::task::spawn_blocking;

use crate::types::AnalysisResult;

/// Analyze a Git repository asynchronously
pub async fn analyze_repo_async(
    path: String,
    branch: String,
    contributor: String,
) -> Result<AnalysisResult, Error> {
    spawn_blocking(move || analyze_repo_with_filter(&path, &branch, &contributor))
        .await
        .map_err(|e| Error::from_str(&e.to_string()))?
}

/// Get list of available branches in the repository
pub fn get_available_branches(repo: &Repository) -> Result<Vec<String>, Error> {
    let mut branch_names = Vec::new();
    let branches = repo.branches(None)?;

    for (branch, _) in branches.flatten() {
        if let Ok(Some(name)) = branch.name() {
            branch_names.push(name.to_string());
        }
    }

    // Sort branches alphabetically
    branch_names.sort();

    // Ensure "main" or "master" is first if present
    if let Some(main_idx) = branch_names.iter().position(|x| x == "main") {
        branch_names.swap(0, main_idx);
    } else if let Some(master_idx) = branch_names.iter().position(|x| x == "master") {
        branch_names.swap(0, master_idx);
    }

    Ok(branch_names)
}

/// Analyze a Git repository with branch and contributor filters
fn analyze_repo_with_filter(
    path: &str,
    branch: &str,
    contributor: &str,
) -> Result<AnalysisResult, Error> {
    let repo = Repository::open(path)?;

    // Get available branches first
    let branches = get_available_branches(&repo)?;

    let mut revwalk = repo.revwalk()?;

    // Set up branch filtering
    if let Ok(branch_ref) = repo.find_branch(branch, git2::BranchType::Local) {
        if let Some(branch_ref_name) = branch_ref.get().name() {
            revwalk.push_ref(branch_ref_name)?;
        } else {
            revwalk.push_head()?;
        }
    } else {
        revwalk.push_head()?;
    }

    let mut commit_count = 0;
    let mut total_lines_added = 0;
    let mut total_lines_deleted = 0;
    let mut author_commit_count: HashMap<String, usize> = HashMap::new();
    let mut commit_activity: Vec<(String, usize, usize)> = Vec::new();

    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let author = commit.author().name().unwrap_or("Unknown").to_string();

        // Skip if not the selected contributor
        if contributor != "All" && author != contributor {
            continue;
        }

        commit_count += 1;
        *author_commit_count.entry(author).or_insert(0) += 1;

        let time = commit.time().seconds();
        let date = DateTime::<Utc>::from_timestamp(time, 0)
            .unwrap_or_default()
            .naive_utc()
            .date()
            .to_string();

        let tree = commit.tree()?;
        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        let mut lines_added = 0;
        let mut lines_deleted = 0;

        if let Some(parent_tree) = parent_tree {
            let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
            let stats = diff.stats()?;
            lines_added = stats.insertions();
            lines_deleted = stats.deletions();
            total_lines_added += lines_added;
            total_lines_deleted += lines_deleted;
        }

        commit_activity.push((date, lines_added, lines_deleted));
    }

    let mut top_contributors: Vec<(String, usize)> =
        author_commit_count.clone().into_iter().collect();
    top_contributors.sort_by(|a, b| b.1.cmp(&a.1));
    top_contributors.truncate(5);

    let average_commit_size = if commit_count > 0 {
        (total_lines_added + total_lines_deleted) as f64 / commit_count as f64
    } else {
        0.0
    };

    let mut commit_frequency: HashMap<String, usize> = HashMap::new();
    for (date, _, _) in &commit_activity {
        let week = date[..7].to_string();
        *commit_frequency.entry(week).or_insert(0) += 1;
    }

    let mut top_contributors_by_lines: Vec<(String, usize)> =
        author_commit_count.into_iter().collect();
    top_contributors_by_lines.sort_by(|a, b| b.1.cmp(&a.1));
    top_contributors_by_lines.truncate(5);

    Ok(AnalysisResult {
        commit_count,
        total_lines_added,
        total_lines_deleted,
        top_contributors,
        commit_activity,
        average_commit_size,
        commit_frequency,
        top_contributors_by_lines,
        available_branches: branches,
    })
}
