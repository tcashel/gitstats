/// Module for Git repository analysis and statistics collection.
/// Provides async functions for analyzing repositories, handling branches, and processing commits.
use crate::types::{AnalysisResult, ProgressEstimate};
use chrono::{DateTime, Utc};
use git2::{Error, Oid, Repository};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;

/// Tuple containing commit statistics (count, lines added, lines deleted)
type CommitData = (usize, usize, usize);
/// Vector of activity data entries (date, lines added, lines deleted)
type ActivityData = Vec<(String, usize, usize)>;
/// Map of contributor names to their commit counts
type ContributorData = HashMap<String, usize>;
/// Combined tuple of all process results
type ProcessResult = (CommitData, ActivityData, ContributorData);
/// Result type for chunk processing operations
type ChunkResult = Result<ProcessResult, Error>;

/// Process a chunk of commits to gather statistics
fn process_commit_chunk(repo: &Repository, chunk: &[Oid], contributor: &str) -> ChunkResult {
    let mut commit_count = 0;
    let mut total_lines_added = 0;
    let mut total_lines_deleted = 0;
    let mut author_commit_count = HashMap::new();
    let mut commit_activity = Vec::with_capacity(chunk.len());

    // Pre-allocate a diff options object to reuse
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts
        .include_untracked(false)
        .ignore_whitespace(true)
        .context_lines(0)
        .ignore_filemode(true)
        .ignore_submodules(true)
        .minimal(true) // Use minimal diff like Git
        .patience(true) // Use patience diff algorithm like Git
        .indent_heuristic(true); // Use indent heuristic like Git

    for &oid in chunk {
        let commit = repo.find_commit(oid)?;
        let author = commit.author().name().unwrap_or("Unknown").to_string();

        if contributor != "All" && author != contributor {
            continue;
        }

        commit_count += 1;
        *author_commit_count.entry(author).or_insert(0) += 1;

        // Use safe timestamp conversion
        let time = commit.time().seconds();
        let date = DateTime::<Utc>::from_timestamp(time, 0)
            .map(|dt| dt.naive_utc().date().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let mut commit_lines_added = 0_usize;
        let mut commit_lines_deleted = 0_usize;

        // Calculate diff stats for the commit
        if let Ok(tree) = commit.tree() {
            let parent_count = commit.parent_count();

            // For non-merge commits or initial commits
            if parent_count <= 1 {
                let parent_tree = if parent_count == 1 {
                    commit.parent(0).ok().and_then(|p| p.tree().ok())
                } else {
                    None
                };

                if let Ok(diff) =
                    repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_opts))
                {
                    // Process each file delta to match git's numstat behavior
                    diff.foreach(
                        &mut |delta, _progress| {
                            // Skip binary files (git shows "-" for these)
                            if delta.flags().contains(git2::DiffFlags::BINARY) {
                                return true;
                            }
                            true
                        },
                        None,
                        Some(&mut |_delta, hunk| {
                            // Count actual line changes
                            commit_lines_added += hunk.new_lines() as usize;
                            commit_lines_deleted += hunk.old_lines() as usize;
                            true
                        }),
                        None,
                    )?;
                }
            } else {
                // For merge commits, compare with each parent and take the maximum
                let mut max_added = 0_usize;
                let mut max_deleted = 0_usize;

                for i in 0..parent_count {
                    let mut parent_added = 0_usize;
                    let mut parent_deleted = 0_usize;

                    if let Ok(parent) = commit.parent(i) {
                        if let Ok(parent_tree) = parent.tree() {
                            if let Ok(diff) = repo.diff_tree_to_tree(
                                Some(&parent_tree),
                                Some(&tree),
                                Some(&mut diff_opts),
                            ) {
                                diff.foreach(
                                    &mut |delta, _progress| {
                                        if delta.flags().contains(git2::DiffFlags::BINARY) {
                                            return true;
                                        }
                                        true
                                    },
                                    None,
                                    Some(&mut |_delta, hunk| {
                                        parent_added += hunk.new_lines() as usize;
                                        parent_deleted += hunk.old_lines() as usize;
                                        true
                                    }),
                                    None,
                                )?;
                            }
                        }
                    }

                    max_added = max_added.max(parent_added);
                    max_deleted = max_deleted.max(parent_deleted);
                }

                commit_lines_added = max_added;
                commit_lines_deleted = max_deleted;
            }
        }

        total_lines_added += commit_lines_added;
        total_lines_deleted += commit_lines_deleted;
        commit_activity.push((date, commit_lines_added, commit_lines_deleted));
    }

    Ok((
        (commit_count, total_lines_added, total_lines_deleted),
        commit_activity,
        author_commit_count,
    ))
}

/// Calculate optimal chunk size for parallel processing based on commit count
fn get_optimal_chunk_size(total_commits: usize) -> usize {
    const MIN_CHUNK_SIZE: usize = 100;
    const MAX_CHUNK_SIZE: usize = 1000;
    let optimal_size = (total_commits / num_cpus::get()).max(MIN_CHUNK_SIZE);
    optimal_size.min(MAX_CHUNK_SIZE)
}

/// Get optimal number of parallel tasks based on system CPU count
fn get_optimal_task_count() -> usize {
    let cpu_count = num_cpus::get();
    (cpu_count * 3 / 4).max(1)
}

/// Process commits in parallel chunks with performance tracking
async fn process_commits_parallel(
    repo_path: std::path::PathBuf,
    commits: Vec<Oid>,
    contributor: String,
    chunk_size: usize,
    progress_tx: Option<mpsc::Sender<ProgressEstimate>>,
) -> Result<(ProcessResult, String), Error> {
    let start_time = Instant::now();
    let total_commits = commits.len();
    let processed_commits = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let chunks: Vec<_> = commits.chunks(chunk_size).collect();
    let mut results = Vec::with_capacity(chunks.len());

    let max_tasks = get_optimal_task_count();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_tasks));

    // Initial progress estimate
    if let Some(tx) = &progress_tx {
        let estimate = ProgressEstimate {
            total_commits,
            processed_commits: 0,
            estimated_total_time: total_commits as f64 / 200.0, // Initial estimate based on benchmarks
            elapsed_time: 0.0,
            commits_per_second: 200.0, // Initial estimate from benchmarks
        };
        let _ = tx.send(estimate).await;
    }

    for chunk in chunks {
        let chunk = chunk.to_vec();
        let chunk_len = chunk.len();
        let repo_path = repo_path.clone();
        let contributor = contributor.clone();
        let processed_commits = Arc::clone(&processed_commits);
        let progress_tx = progress_tx.clone();
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e: tokio::sync::AcquireError| Error::from_str(&e.to_string()))?;

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let result = spawn_blocking(
                move || -> Result<(CommitData, ActivityData, ContributorData), Error> {
                    let repo = Repository::open(repo_path)?;
                    process_commit_chunk(&repo, &chunk, &contributor)
                },
            )
            .await
            .map_err(|e: tokio::task::JoinError| Error::from_str(&e.to_string()))?;

            // Update progress after each chunk
            let current = processed_commits
                .fetch_add(chunk_len, std::sync::atomic::Ordering::SeqCst)
                + chunk_len;
            if let Some(tx) = &progress_tx {
                let elapsed = start_time.elapsed().as_secs_f64();
                let commits_per_second = current as f64 / elapsed;
                let estimate = ProgressEstimate {
                    total_commits,
                    processed_commits: current,
                    estimated_total_time: total_commits as f64 / commits_per_second,
                    elapsed_time: elapsed,
                    commits_per_second,
                };
                let _ = tx.send(estimate).await;
            }

            result
        });
        results.push(handle);
    }

    let mut total_commit_count = 0;
    let mut total_lines_added = 0;
    let mut total_lines_deleted = 0;
    let mut total_commit_activity = Vec::with_capacity(commits.len());
    let mut total_author_count = HashMap::new();

    for handle in results {
        match handle.await {
            Ok(Ok(((commits, lines_added, lines_deleted), activity, authors))) => {
                total_commit_count += commits;
                total_lines_added += lines_added;
                total_lines_deleted += lines_deleted;
                total_commit_activity.extend(activity);
                for (author, count) in authors {
                    *total_author_count.entry(author).or_insert(0) += count;
                }
            }
            Ok(Err(e)) => eprintln!("Error processing commit chunk: {}", e),
            Err(e) => eprintln!("Task join error: {}", e),
        }
    }

    let elapsed = start_time.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let commits_per_sec = total_commits as f64 / elapsed_secs;

    let stats = format!(
        "Processed {} commits in {:.2}s\nCommits/sec: {:.1}\nChunk size: {}\nParallel tasks: {}",
        total_commits, elapsed_secs, commits_per_sec, chunk_size, max_tasks
    );

    Ok((
        (
            (total_commit_count, total_lines_added, total_lines_deleted),
            total_commit_activity,
            total_author_count,
        ),
        stats,
    ))
}

/// Analyze a Git repository with branch and contributor filters
async fn analyze_repo_with_filter(
    repo: Repository,
    branch: &str,
    contributor: &str,
    progress_tx: Option<mpsc::Sender<ProgressEstimate>>,
) -> Result<AnalysisResult, Error> {
    let start_time = Instant::now();
    let repo_path = repo.path().to_path_buf();

    // Get all commits
    let commits: Vec<Oid> = {
        let repo_path = repo_path.clone();
        let branch = branch.to_string();
        spawn_blocking(move || -> Result<Vec<Oid>, Error> {
            let repo = Repository::open(&repo_path)?;
            let mut revwalk = repo.revwalk()?;

            // Try to use the specified branch, fallback to HEAD
            if let Ok(branch_ref) = repo.find_branch(&branch, git2::BranchType::Local) {
                if let Some(branch_ref_name) = branch_ref.get().name() {
                    revwalk.push_ref(branch_ref_name)?;
                } else {
                    revwalk.push_head()?;
                }
            } else {
                revwalk.push_head()?;
            }

            revwalk.collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e: tokio::task::JoinError| Error::from_str(&e.to_string()))?
        .map_err(|e: Error| Error::from_str(&e.to_string()))?
    };

    let chunk_size = get_optimal_chunk_size(commits.len());
    let ((commit_stats, commit_activity, author_commit_count), stats) = process_commits_parallel(
        repo_path.clone(),
        commits,
        contributor.to_string(),
        chunk_size,
        progress_tx,
    )
    .await?;

    let (commit_count, total_lines_added, total_lines_deleted) = commit_stats;

    let mut top_contributors: Vec<(String, usize)> = author_commit_count
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    top_contributors.sort_by(|a, b| b.1.cmp(&a.1));
    top_contributors.truncate(5);

    let average_commit_size = if commit_count > 0 {
        (total_lines_added + total_lines_deleted) as f64 / commit_count as f64
    } else {
        0.0
    };

    let mut commit_frequency = HashMap::new();
    for (date, _, _) in &commit_activity {
        let week = date[..7].to_string();
        *commit_frequency.entry(week).or_insert(0) += 1;
    }

    // Get available branches
    let branch_names = {
        let repo_path = repo_path.clone();
        spawn_blocking(move || -> Result<Vec<String>, Error> {
            let repo = Repository::open(repo_path)?;
            let mut branch_names = Vec::new();
            let branches = repo.branches(None)?;

            for branch in branches.flatten() {
                if let Ok(Some(name)) = branch.0.name() {
                    branch_names.push(name.to_string());
                }
            }

            branch_names.sort();
            if let Some(main_idx) = branch_names.iter().position(|x| x == "main") {
                branch_names.swap(0, main_idx);
            } else if let Some(master_idx) = branch_names.iter().position(|x| x == "master") {
                branch_names.swap(0, master_idx);
            }

            Ok(branch_names)
        })
        .await
        .map_err(|e: tokio::task::JoinError| Error::from_str(&e.to_string()))?
        .map_err(|e: Error| Error::from_str(&e.to_string()))?
    };

    let elapsed = start_time.elapsed();

    Ok(AnalysisResult {
        commit_count,
        total_lines_added,
        total_lines_deleted,
        top_contributors: top_contributors.clone(),
        commit_activity,
        average_commit_size,
        commit_frequency,
        top_contributors_by_lines: top_contributors,
        available_branches: branch_names,
        elapsed_time: elapsed.as_secs_f64(),
        processing_stats: stats,
    })
}

/// Analyze a Git repository asynchronously with specified branch and contributor filters
pub async fn analyze_repo_async(
    path: String,
    branch: String,
    contributor: String,
    progress_tx: Option<mpsc::Sender<ProgressEstimate>>,
) -> Result<AnalysisResult, Error> {
    let repo = spawn_blocking(move || -> Result<Repository, Error> { Repository::open(&path) })
        .await
        .map_err(|e: tokio::task::JoinError| Error::from_str(&e.to_string()))?
        .map_err(|e: Error| Error::from_str(&e.to_string()))?;

    analyze_repo_with_filter(repo, &branch, &contributor, progress_tx).await
}

/// Get list of available branches in the repository
pub async fn get_available_branches(repo: &Repository) -> Result<Vec<String>, Error> {
    let repo_path = repo.path().to_path_buf();

    spawn_blocking(move || -> Result<Vec<String>, Error> {
        let repo = Repository::open(repo_path)?;
        let mut branch_names = Vec::new();
        let branches = repo.branches(None)?;

        for branch in branches.flatten() {
            if let Ok(Some(name)) = branch.0.name() {
                branch_names.push(name.to_string());
            }
        }

        branch_names.sort();
        if let Some(main_idx) = branch_names.iter().position(|x| x == "main") {
            branch_names.swap(0, main_idx);
        } else if let Some(master_idx) = branch_names.iter().position(|x| x == "master") {
            branch_names.swap(0, master_idx);
        }

        Ok(branch_names)
    })
    .await
    .map_err(|e: tokio::task::JoinError| Error::from_str(&e.to_string()))?
}
