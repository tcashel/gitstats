/// Integration tests for the GitStats application.
/// Tests the full workflow from repository analysis to plot generation.
use git2::{Repository, Signature};
use gitstats::app::App;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// Set up a test Git repository with sample commits and files
///
/// # Returns
/// * `(TempDir, Repository)` - Temporary directory and initialized repository
fn setup_test_repo() -> (TempDir, Repository) {
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

    // Add some test files and commits
    let files = vec![
        ("file1.txt", "Hello\nWorld\n"),
        ("file2.txt", "Test\nContent\n"),
        ("src/main.rs", "fn main() {\n    println!(\"Hello\");\n}\n"),
    ];

    for (file_name, content) in files {
        let file_path = temp_dir.path().join(file_name);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new(file_name)).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();

        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("Add {}", file_name),
                &tree,
                &[&parent],
            )
            .unwrap();
        }
    }

    (temp_dir, repo)
}

/// Set up a test application instance with sample data
///
/// # Returns
/// * `App` - Initialized application instance with test data
fn setup_test_app() -> App {
    let mut app = App::default();
    app.plot_path = "test_plot.png".to_string();
    app.commit_activity = vec![
        ("2023-01-01".to_string(), 10, 5),
        ("2023-01-02".to_string(), 15, 8),
        ("2023-01-03".to_string(), 20, 10),
    ];
    app
}

/// Test the complete application workflow
/// Tests repository analysis, branch selection, plot generation, and caching
#[tokio::test]
async fn test_full_workflow() {
    let (temp_dir, _repo) = setup_test_repo();

    // Initialize app
    let app = Arc::new(Mutex::new(App::default()));
    {
        if let Ok(mut app) = app.lock() {
            app.repo_path = temp_dir.path().to_str().unwrap().to_string();
        }
    }

    // Test repository analysis
    {
        if let Ok(mut app) = app.lock() {
            assert_eq!(app.commit_count, 0);
            assert!(app.top_contributors.is_empty());

            // Analyze repository
            let result = gitstats::analysis::analyze_repo_async(
                app.repo_path.clone(),
                "main".to_string(),
                "All".to_string(),
                None,
            )
            .await
            .unwrap();

            app.update_with_result(result);

            // Verify analysis results
            assert!(app.commit_count > 0);
            assert!(!app.top_contributors.is_empty());
            assert!(app.total_lines_added > 0);
            assert_eq!(app.selected_contributor, "All");
        }
    }

    // Test branch selection
    {
        if let Ok(mut app) = app.lock() {
            assert!(!app.available_branches.is_empty());
            let original_branch = app.selected_branch.clone();

            // Switch branch
            if let Some(branch) = app.available_branches.get(0) {
                app.selected_branch = branch.clone();
                assert_ne!(app.selected_branch, original_branch);
            }
        }
    }

    // Test plot generation
    {
        let mut app = app.lock().unwrap();
        let plot_path = temp_dir
            .path()
            .join("test_plot.png")
            .to_str()
            .unwrap()
            .to_string();
        app.plot_path = plot_path.clone();
        app.commit_activity = vec![
            ("2023-01-01".to_string(), 10, 5),
            ("2023-01-02".to_string(), 15, 8),
            ("2023-01-03".to_string(), 20, 10),
        ];

        // Test different metrics
        for metric in &["Commits", "Code Changes", "Code Frequency"] {
            let mut app_clone = App::default();
            app_clone.plot_path = plot_path.clone();
            app_clone.commit_activity = app.commit_activity.clone();
            app_clone.current_metric = metric.to_string();

            // Generate the plot and verify we get data back
            let plot_result = gitstats::plotting::generate_plot_async(app_clone).await;
            assert!(
                plot_result.is_ok(),
                "Failed to generate plot for metric {}: {:?}",
                metric,
                plot_result
            );

            let plot_data = plot_result.unwrap();
            assert!(
                !plot_data.is_empty(),
                "Plot data is empty for metric {}",
                metric
            );

            // Verify it's valid RGBA data (4 bytes per pixel)
            let width = 640;
            let height = 480;
            let expected_size = width * height * 4;
            assert_eq!(
                plot_data.len(),
                expected_size,
                "Invalid RGBA data size for metric {}. Expected {} bytes, got {}",
                metric,
                expected_size,
                plot_data.len()
            );

            // Verify first pixel is in RGBA format (4 bytes)
            assert_eq!(
                plot_data.len() >= 4,
                true,
                "Plot data too short for metric {}",
                metric
            );
        }
    }

    // Test contributor filtering
    {
        let mut app = app.lock().unwrap();
        if let Some((contributor, _)) = app.top_contributors.first() {
            let contributor = contributor.clone();
            app.selected_contributor = contributor.clone();

            // Analyze with contributor filter
            let result = gitstats::analysis::analyze_repo_async(
                app.repo_path.clone(),
                app.selected_branch.clone(),
                contributor,
                None,
            )
            .await
            .unwrap();

            app.update_with_result(result);
            assert!(app.commit_count > 0);
        }
    }

    // Test caching
    {
        let cache_key = {
            let app = app.lock().unwrap();
            gitstats::types::CacheKey {
                branch: app.selected_branch.clone(),
                contributor: app.selected_contributor.clone(),
            }
        };

        let app = app.lock().unwrap();
        assert!(app
            .get_cached_result(&cache_key.branch, &cache_key.contributor)
            .is_some());
    }
}

/// Test error handling in repository operations
/// Tests invalid repository paths and branch names
#[tokio::test]
async fn test_error_handling() {
    let app = Arc::new(Mutex::new(App::default()));

    // Test invalid repository path
    {
        let mut app = app.lock().unwrap();
        app.repo_path = "/nonexistent/path".to_string();

        let result = gitstats::analysis::analyze_repo_async(
            app.repo_path.clone(),
            "main".to_string(),
            "All".to_string(),
            None,
        )
        .await;

        assert!(result.is_err());
    }

    // Test invalid branch
    {
        let (temp_dir, _repo) = setup_test_repo();
        let mut app = app.lock().unwrap();
        app.repo_path = temp_dir.path().to_str().unwrap().to_string();

        let result = gitstats::analysis::analyze_repo_async(
            app.repo_path.clone(),
            "nonexistent-branch".to_string(),
            "All".to_string(),
            None,
        )
        .await;

        // Should fall back to HEAD
        assert!(result.is_ok());
    }
}

/// Test plot generation functionality
/// Tests generation of plots with sample data
#[tokio::test]
async fn test_plot_generation() {
    let app = setup_test_app();
    assert!(gitstats::plotting::generate_plot_async(app).await.is_ok());
}
