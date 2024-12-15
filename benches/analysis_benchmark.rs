/// Benchmark module for testing performance of Git analysis and plotting operations.
/// This module compares our implementation against native git commands and tests
/// both fast path (using git's internal files) and full path (walking commits) operations.
/// 
/// The benchmarks use both:
/// 1. A synthetic test repository (setup_large_test_repo)
/// 2. A real-world repository (ripgrep)
/// 
/// Key benchmark categories:
/// - Git operations: Native git command performance
/// - Fast path: Our optimized implementation using git's internal files
/// - Full path: Our complete implementation walking the commit tree
use criterion::{criterion_group, criterion_main, Criterion};
use git2::{Repository, Signature};
use gitstats::analysis::analyze_repo_async;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Run git command and parse output
/// Used as the baseline for performance comparison
fn run_git_command(repo_path: &Path, args: &[&str]) -> String {
    Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default()
}

/// Get commit count using raw git command
/// Uses 'git rev-list --count' which is the fastest native method
fn get_git_commit_count(repo_path: &Path, branch: &str) -> usize {
    let output = run_git_command(repo_path, &["rev-list", "--count", branch]);
    output.trim().parse().unwrap_or(0)
}

/// Get contributor stats using raw git command
/// Uses 'git shortlog' which matches our contributor stats implementation
fn get_git_contributor_stats(repo_path: &Path, branch: &str) -> Vec<(String, usize)> {
    let output = run_git_command(
        repo_path,
        &["shortlog", "-sn", "--all", branch], // Removed --no-merges to match our implementation
    );
    output
        .lines()
        .map(|line| {
            let parts: Vec<_> = line.trim().splitn(2, '\t').collect();
            if parts.len() == 2 {
                (
                    parts[1].to_string(),
                    parts[0].trim().parse().unwrap_or(0),
                )
            } else {
                ("Unknown".to_string(), 0)
            }
        })
        .collect()
}

/// Get commit stats using raw git command
/// Uses 'git log --numstat' to match our line counting implementation
fn get_git_commit_stats(repo_path: &Path, branch: &str) -> String {
    run_git_command(repo_path, &["log", "--numstat", branch])
}

/// Get filtered stats using raw git command
/// Matches our filtered stats implementation
fn get_git_filtered_stats(repo_path: &Path, author: &str) -> String {
    run_git_command(
        repo_path,
        &[
            "log",
            "--numstat",
            "--author",
            author,
            "--no-merges", // We do filter merges in filtered stats
        ],
    )
}

/// Set up real-world repository for benchmarking
/// Uses ripgrep as a representative real-world Rust project
/// - Medium size (not too large to clone quickly)
/// - Active development (good commit history)
/// - Multiple contributors
fn setup_real_world_repo() -> (TempDir, Repository) {
    let temp_dir = TempDir::new().unwrap();
    println!("Cloning ripgrep repository...");
    
    Command::new("git")
        .current_dir(temp_dir.path())
        .args(&["clone", "https://github.com/BurntSushi/ripgrep.git", "."])
        .status()
        .expect("Failed to clone ripgrep repository");

    let repo = Repository::open(temp_dir.path()).unwrap();
    println!("Repository setup complete for ripgrep");
    (temp_dir, repo)
}

/// Get the most active contributor for a repository
fn get_most_active_contributor(repo_path: &Path) -> String {
    let output = run_git_command(
        repo_path,
        &["shortlog", "-sn", "--all", "--no-merges", "HEAD"],
    );
    output
        .lines()
        .next()
        .and_then(|line| line.splitn(2, '\t').nth(1))
        .map(|name| name.trim().to_string())
        .unwrap_or_else(|| "Andrew Gallant".to_string()) // Fallback to ripgrep's main author
}

/// Set up a large test repository for benchmarking
/// Creates a repository with multiple commits and files
///
/// # Returns
/// * `(TempDir, Repository)` - Temporary directory and initialized repository
fn setup_large_test_repo() -> (TempDir, Repository) {
    let temp_dir = TempDir::new().unwrap();
    let repo = Repository::init(temp_dir.path()).unwrap();

    // Create initial commit
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };

    {
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )
        .unwrap();
    }

    // Create multiple files and commits to simulate a large repository
    for i in 0..100 {
        let file_name = format!("file_{}.txt", i);
        let content = format!("Content for file {}\n", i);
        let file_path = temp_dir.path().join(&file_name);
        fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new(&file_name)).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();

        // Alternate between different authors
        let author = if i % 2 == 0 {
            Signature::now("Test User", "test@example.com").unwrap()
        } else {
            Signature::now("Another User", "another@example.com").unwrap()
        };

        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(
                Some("HEAD"),
                &author,
                &author,
                &format!("Add {}", file_name),
                &tree,
                &[&parent],
            )
            .unwrap();
        }
    }

    // Create develop branch with its own commits
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let develop = repo.branch("develop", &head, false).unwrap();
        let develop_ref = develop.get().name().unwrap().to_string();

        // Add some commits to develop
        for i in 0..20 {
            let file_name = format!("develop_file_{}.txt", i);
            let content = format!("Develop content {}\n", i);
            let file_path = temp_dir.path().join(&file_name);
            fs::write(&file_path, content).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new(&file_name)).unwrap();
            index.write().unwrap();

            let tree_id = index.write_tree().unwrap();
            let parent = repo
                .find_reference(&develop_ref)
                .unwrap()
                .peel_to_commit()
                .unwrap();

            {
                let tree = repo.find_tree(tree_id).unwrap();
                repo.commit(
                    Some(&develop_ref),
                    &signature,
                    &signature,
                    &format!("Add develop file {}", i),
                    &tree,
                    &[&parent],
                )
                .unwrap();
            }
        }
    }

    (temp_dir, repo)
}

/// Benchmark repository analysis operations
/// Compares performance between:
/// 1. Native git commands (baseline)
/// 2. Our fast path implementation (using git's internal files)
/// 3. Our full path implementation (walking commits)
/// 
/// Tests the following operations:
/// - count_commits_git: Native git commit counting
/// - count_commits_ours_fast: Our fast path using git's internal files
/// - count_commits_ours_full: Our full implementation walking commits
/// - contributor_stats_git: Native git contributor statistics
/// - contributor_stats_ours_fast: Our fast path contributor stats
/// - contributor_stats_ours_full: Our full path contributor stats
/// - commit_stats_git: Native git commit statistics
/// - commit_stats_ours_full: Our full commit stats (no fast path available)
/// - filtered_stats_git: Native git filtered statistics
/// - filtered_stats_ours_full: Our filtered stats (no fast path available)
fn bench_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("git_comparison");
    let rt = Runtime::new().unwrap();
    
    // Set up real-world repo
    let (real_dir, _real_repo) = setup_real_world_repo();
    let test_contributor = get_most_active_contributor(real_dir.path());
    println!("Using contributor: {}", test_contributor);

    // Basic commit count comparison
    group.bench_function("count_commits_git", |b| {
        b.iter(|| get_git_commit_count(real_dir.path(), "HEAD"));
    });

    // Fast path uses git's internal files
    group.bench_function("count_commits_ours_fast", |b| {
        b.iter(|| {
            let result = rt.block_on(async {
                analyze_repo_async(
                    real_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    "All".to_string(), // "All" triggers fast path
                )
                .await
                .unwrap()
            });
            assert!(result.commit_count > 0);
        });
    });

    // Full path walks all commits
    group.bench_function("count_commits_ours_full", |b| {
        b.iter(|| {
            let result = rt.block_on(async {
                analyze_repo_async(
                    real_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    test_contributor.clone(), // Clone for owned String
                )
                .await
                .unwrap()
            });
            assert!(result.commit_count > 0);
        });
    });

    // Contributor stats comparison
    group.bench_function("contributor_stats_git", |b| {
        b.iter(|| get_git_contributor_stats(real_dir.path(), "HEAD"));
    });

    group.bench_function("contributor_stats_ours_fast", |b| {
        b.iter(|| {
            let result = rt.block_on(async {
                analyze_repo_async(
                    real_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    "All".to_string(), // "All" triggers fast path
                )
                .await
                .unwrap()
            });
            assert!(!result.top_contributors.is_empty());
        });
    });

    group.bench_function("contributor_stats_ours_full", |b| {
        b.iter(|| {
            let result = rt.block_on(async {
                analyze_repo_async(
                    real_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    test_contributor.clone(), // Clone for owned String
                )
                .await
                .unwrap()
            });
            assert!(!result.top_contributors.is_empty());
        });
    });

    // Full commit stats comparison (always uses full path since we need line stats)
    group.bench_function("commit_stats_git", |b| {
        b.iter(|| get_git_commit_stats(real_dir.path(), "HEAD"));
    });

    group.bench_function("commit_stats_ours_full", |b| {
        b.iter(|| {
            let result = rt.block_on(async {
                analyze_repo_async(
                    real_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    "All".to_string(),
                )
                .await
                .unwrap()
            });
            assert!(result.commit_count > 0);
        });
    });

    // Filtered stats comparison (always uses full path)
    group.bench_function("filtered_stats_git", |b| {
        b.iter(|| {
            get_git_filtered_stats(real_dir.path(), &test_contributor)
        });
    });

    group.bench_function("filtered_stats_ours_full", |b| {
        b.iter(|| {
            let result = rt.block_on(async {
                analyze_repo_async(
                    real_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    test_contributor.clone(), // Clone for owned String
                )
                .await
                .unwrap()
            });
            assert!(result.commit_count > 0);
        });
    });

    group.finish();
}

/// Benchmark plot generation operations
/// Tests the performance of different visualization types and options:
/// 
/// Plot types:
/// - plot_commits: Commit frequency over time
/// - plot_code_changes: Lines added/deleted over time
/// - plot_with_log_scale: Code changes with logarithmic scaling
/// 
/// Each benchmark measures:
/// - Data preparation time
/// - Plot generation time
/// - File writing time
fn bench_plotting(c: &mut Criterion) {
    let mut group = c.benchmark_group("plotting");
    let rt = Runtime::new().unwrap();
    let (temp_dir, _repo) = setup_large_test_repo();

    let result = rt.block_on(async {
        analyze_repo_async(
            temp_dir.path().to_str().unwrap().to_string(),
            "main".to_string(),
            "All".to_string(),
        )
        .await
        .unwrap()
    });

    // Commits plot
    {
        let mut app = gitstats::GitStatsApp::default();
        app.update_with_result(result.clone());
        app.plot_path = temp_dir
            .path()
            .join("bench_plot.png")
            .to_str()
            .unwrap()
            .to_string();
        app.current_metric = "Commits".to_string();

        group.bench_function("plot_commits", |b| {
            let app = app.clone();
            b.iter(|| {
                rt.block_on(async {
                    gitstats::plotting::generate_plot_async(app.clone())
                        .await
                        .unwrap()
                })
            });
        });
    }

    // Code changes plot
    {
        let mut app = gitstats::GitStatsApp::default();
        app.update_with_result(result.clone());
        app.plot_path = temp_dir
            .path()
            .join("bench_plot.png")
            .to_str()
            .unwrap()
            .to_string();
        app.current_metric = "Code Changes".to_string();

        group.bench_function("plot_code_changes", |b| {
            let app = app.clone();
            b.iter(|| {
                rt.block_on(async {
                    gitstats::plotting::generate_plot_async(app.clone())
                        .await
                        .unwrap()
                })
            });
        });
    }

    // Log scale plot
    {
        let mut app = gitstats::GitStatsApp::default();
        app.update_with_result(result);
        app.plot_path = temp_dir
            .path()
            .join("bench_plot.png")
            .to_str()
            .unwrap()
            .to_string();
        app.current_metric = "Code Changes".to_string();
        app.use_log_scale = true;

        group.bench_function("plot_with_log_scale", |b| {
            let app = app.clone();
            b.iter(|| {
                rt.block_on(async {
                    gitstats::plotting::generate_plot_async(app.clone())
                        .await
                        .unwrap()
                })
            });
        });
    }

    group.finish();
}

/// Benchmark caching operations
/// Tests the performance of the result caching system:
/// 
/// Operations tested:
/// - cache_lookup: Time to retrieve cached analysis results
/// - Uses CacheKey combining branch and contributor
/// 
/// Cache characteristics:
/// - In-memory storage
/// - Key-based lookup
/// - Complete result storage
fn bench_caching(c: &mut Criterion) {
    let mut group = c.benchmark_group("caching");
    let rt = Runtime::new().unwrap();
    let (temp_dir, _repo) = setup_large_test_repo();

    let mut app = gitstats::GitStatsApp::default();
    app.repo_path = temp_dir.path().to_str().unwrap().to_string();

    // Pre-populate cache
    let result = rt.block_on(async {
        analyze_repo_async(app.repo_path.clone(), "main".to_string(), "All".to_string())
            .await
            .unwrap()
    });
    app.update_with_result(result);

    group.bench_function("cache_lookup", |b| {
        b.iter(|| {
            let cache_key = gitstats::types::CacheKey {
                branch: "main".to_string(),
                contributor: "All".to_string(),
            };
            app.get_cached_result(&cache_key.branch, &cache_key.contributor)
        });
    });

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(50)  // Keep 50 samples for statistical significance
        .measurement_time(std::time::Duration::from_secs(15)); // Increase time limit to 15 seconds
    targets = bench_analysis, bench_plotting, bench_caching
);
criterion_main!(benches);
