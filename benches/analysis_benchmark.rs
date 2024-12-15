/// Benchmark module for testing performance of Git analysis and plotting operations.
/// Measures performance of repository analysis, plot generation, and caching.
use criterion::{criterion_group, criterion_main, Criterion};
use git2::{Repository, Signature};
use gitstats::analysis::analyze_repo_async;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use tokio::runtime::Runtime;

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
/// Tests performance of commit processing and data aggregation
///
/// # Arguments
/// * `c` - Criterion benchmark configuration
fn bench_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("repository_analysis");
    let rt = Runtime::new().unwrap();

    group.bench_function("analyze_full_repo", |b| {
        let (temp_dir, _repo) = setup_large_test_repo();
        b.iter(|| {
            rt.block_on(async {
                analyze_repo_async(
                    temp_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    "All".to_string(),
                )
                .await
                .unwrap()
            })
        });
    });

    group.bench_function("analyze_filtered_repo", |b| {
        let (temp_dir, _repo) = setup_large_test_repo();
        b.iter(|| {
            rt.block_on(async {
                analyze_repo_async(
                    temp_dir.path().to_str().unwrap().to_string(),
                    "main".to_string(),
                    "Test User".to_string(),
                )
                .await
                .unwrap()
            })
        });
    });

    group.bench_function("analyze_develop_branch", |b| {
        let (temp_dir, _repo) = setup_large_test_repo();
        b.iter(|| {
            rt.block_on(async {
                analyze_repo_async(
                    temp_dir.path().to_str().unwrap().to_string(),
                    "develop".to_string(),
                    "All".to_string(),
                )
                .await
                .unwrap()
            })
        });
    });

    group.finish();
}

/// Benchmark plot generation operations
/// Tests performance of different plot types and scaling options
///
/// # Arguments
/// * `c` - Criterion benchmark configuration
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
/// Tests performance of result caching and retrieval
///
/// # Arguments
/// * `c` - Criterion benchmark configuration
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
    config = Criterion::default();
    targets = bench_analysis, bench_plotting, bench_caching
);
criterion_main!(benches);
