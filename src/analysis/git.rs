use chrono::{DateTime, Utc};
use git2::{Error, Repository, Oid};
use std::collections::HashMap;
use tokio::task::spawn_blocking;
use std::time::Instant;

use crate::types::AnalysisResult;

/// Analyze a Git repository asynchronously
pub async fn analyze_repo_async(
    path: String,
    branch: String,
    contributor: String,
) -> Result<AnalysisResult, Error> {
    // Open repository in a blocking task since git2 operations are blocking
    let repo = spawn_blocking(move || Repository::open(&path))
        .await
        .map_err(|e| Error::from_str(&e.to_string()))?
        .map_err(|e| Error::from_str(&e.to_string()))?;

    analyze_repo_with_filter(repo, &branch, &contributor).await
}

/// Get list of available branches in the repository
pub async fn get_available_branches(repo: &Repository) -> Result<Vec<String>, Error> {
    let repo_path = repo.path().to_path_buf();
    
    // Move git operations to a blocking task with a new repo instance
    spawn_blocking(move || {
        let repo = Repository::open(repo_path)?;
        let mut branch_names = Vec::new();
        let branches = repo.branches(None)?;

        for (branch, _) in branches.flatten() {
            if let Ok(Some(name)) = branch.name() {
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
    .map_err(|e| Error::from_str(&e.to_string()))?
}

/// Process a chunk of commits
fn process_commit_chunk(
    repo: &Repository,
    chunk: &[Oid],
    contributor: &str,
) -> Result<(usize, usize, usize, Vec<(String, usize, usize)>, HashMap<String, usize>), Error> {
    let mut commit_count = 0;
    let mut total_lines_added = 0;
    let mut total_lines_deleted = 0;
    let mut author_commit_count: HashMap<String, usize> = HashMap::new();
    let mut commit_activity: Vec<(String, usize, usize)> = Vec::with_capacity(chunk.len());

    // Pre-allocate a diff options object to reuse
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(false)
             .ignore_whitespace(true)
             .context_lines(0);

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

        // Optimize tree diffing
        if let (Ok(tree), Some(Ok(parent_tree))) = (
            commit.tree(),
            commit.parent_count().checked_sub(1).and_then(|_| {
                commit.parent(0).ok().map(|parent| parent.tree())
            }),
        ) {
            if let Ok(diff) = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut diff_opts)) {
                if let Ok(stats) = diff.stats() {
                    let lines_added = stats.insertions();
                    let lines_deleted = stats.deletions();
                    total_lines_added += lines_added;
                    total_lines_deleted += lines_deleted;
                    commit_activity.push((date, lines_added, lines_deleted));
                }
            }
        }
    }

    // Optimize memory usage by shrinking vectors if they're much larger than needed
    if commit_activity.capacity() > commit_activity.len() * 2 {
        commit_activity.shrink_to_fit();
    }

    Ok((
        commit_count,
        total_lines_added,
        total_lines_deleted,
        commit_activity,
        author_commit_count,
    ))
}

/// Get optimal chunk size based on commit count
fn get_optimal_chunk_size(_total_commits: usize) -> usize {
    // Aim for chunks that will take ~100ms to process
    const TARGET_CHUNK_TIME_MS: usize = 100;
    const COMMITS_PER_MS: usize = 5; // Estimated commits processable per millisecond
    const MIN_CHUNK_SIZE: usize = 100;
    const MAX_CHUNK_SIZE: usize = 2000;

    let optimal_size = TARGET_CHUNK_TIME_MS * COMMITS_PER_MS;
    optimal_size.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE)
}

/// Get optimal number of parallel tasks based on system resources
fn get_optimal_task_count() -> usize {
    let cpu_count = num_cpus::get();
    // Use 75% of available CPUs to leave room for other system processes
    (cpu_count * 3 / 4).max(1)
}

/// Process commits in parallel chunks
async fn process_commits_parallel(
    repo_path: std::path::PathBuf,
    commits: Vec<Oid>,
    contributor: String,
    _chunk_size: usize,
) -> Result<(usize, usize, usize, Vec<(String, usize, usize)>, HashMap<String, usize>, String), Error> {
    let start_time = Instant::now();
    let total_commits = commits.len();

    let optimal_chunk_size = get_optimal_chunk_size(commits.len());
    let chunks: Vec<_> = commits.chunks(optimal_chunk_size).collect();
    let mut results = Vec::with_capacity(chunks.len());

    // Process chunks in parallel using a bounded number of tasks
    let max_tasks = get_optimal_task_count();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_tasks));

    for chunk in chunks {
        let chunk = chunk.to_vec();
        let repo_path = repo_path.clone();
        let contributor = contributor.clone();
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(e) => return Err(Error::from_str(&format!("Failed to acquire semaphore: {}", e))),
        };

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let result = spawn_blocking(move || {
                let repo = Repository::open(repo_path)?;
                process_commit_chunk(&repo, &chunk, &contributor)
            })
            .await
            .map_err(|e| Error::from_str(&e.to_string()))?;

            result
        });
        results.push(handle);
    }

    // Wait for all tasks and collect results
    let mut total_commit_count = 0;
    let mut total_lines_added = 0;
    let mut total_lines_deleted = 0;
    let mut total_commit_activity = Vec::with_capacity(commits.len());
    let mut total_author_count = HashMap::new();

    for handle in results {
        match handle.await {
            Ok(Ok((commits, lines_added, lines_deleted, activity, authors))) => {
                total_commit_count += commits;
                total_lines_added += lines_added;
                total_lines_deleted += lines_deleted;
                total_commit_activity.extend(activity);
                for (author, count) in authors {
                    *total_author_count.entry(author).or_insert(0) += count;
                }
            }
            Ok(Err(e)) => {
                eprintln!("Error processing commit chunk: {}", e);
            }
            Err(e) => {
                eprintln!("Task join error: {}", e);
            }
        }
    }

    let elapsed = start_time.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let commits_per_sec = total_commits as f64 / elapsed_secs;

    let stats = format!(
        "Processed {} commits in {:.2}s\nCommits/sec: {:.1}\nChunk size: {}\nParallel tasks: {}",
        total_commits,
        elapsed_secs,
        commits_per_sec,
        optimal_chunk_size,
        max_tasks
    );

    Ok((
        total_commit_count,
        total_lines_added,
        total_lines_deleted,
        total_commit_activity,
        total_author_count,
        stats,
    ))
}

/// Analyze a Git repository with branch and contributor filters
async fn analyze_repo_with_filter(
    repo: Repository,
    branch: &str,
    contributor: &str,
) -> Result<AnalysisResult, Error> {
    let start_time = Instant::now();
    let repo_path = repo.path().to_path_buf();
    let branch = branch.to_string();
    let contributor = contributor.to_string();

    // Get commits in a blocking task
    let commits = {
        let repo_path = repo_path.clone();
        spawn_blocking(move || {
            let repo = Repository::open(&repo_path)?;
            let mut revwalk = repo.revwalk()?;
            
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
        .map_err(|e| Error::from_str(&e.to_string()))?
        .map_err(|e| Error::from_str(&e.to_string()))?
    };

    // Process commits in parallel chunks
    let (commit_count, total_lines_added, total_lines_deleted, commit_activity, author_commit_count, stats) =
        process_commits_parallel(repo_path.clone(), commits, contributor.clone(), 1000).await?;

    let mut top_contributors: Vec<(String, usize)> = author_commit_count.clone().into_iter().collect();
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

    let mut top_contributors_by_lines = top_contributors.clone();
    top_contributors_by_lines.sort_by(|a, b| b.1.cmp(&a.1));
    top_contributors_by_lines.truncate(5);

    // Get branches in a separate blocking task
    let branch_names = {
        let repo_path = repo_path.clone();
        spawn_blocking(move || {
            let repo = Repository::open(repo_path)?;
            let mut branch_names = Vec::new();
            let branches = repo.branches(None)?;
            for (branch, _) in branches.flatten() {
                if let Ok(Some(name)) = branch.name() {
                    branch_names.push(name.to_string());
                }
            }
            branch_names.sort();
            if let Some(main_idx) = branch_names.iter().position(|x| x == "main") {
                branch_names.swap(0, main_idx);
            } else if let Some(master_idx) = branch_names.iter().position(|x| x == "master") {
                branch_names.swap(0, master_idx);
            }
            Ok::<_, Error>(branch_names)
        })
        .await
        .map_err(|e| Error::from_str(&e.to_string()))?
        .map_err(|e| Error::from_str(&e.to_string()))?
    };

    let elapsed = start_time.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();

    Ok(AnalysisResult {
        commit_count,
        total_lines_added,
        total_lines_deleted,
        top_contributors,
        commit_activity,
        average_commit_size,
        commit_frequency,
        top_contributors_by_lines,
        available_branches: branch_names,
        elapsed_time: elapsed_secs,
        processing_stats: stats,
    })
}
