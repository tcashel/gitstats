use gitstats::app::App;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;

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
        ).unwrap();
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
            ).unwrap();
        }
    }

    (temp_dir, repo)
}

#[tokio::test]
async fn test_full_workflow() {
    let (temp_dir, _repo) = setup_test_repo();
    
    // Initialize app
    let app = Arc::new(Mutex::new(App::default()));
    {
        let mut app = app.lock().unwrap();
        app.repo_path = temp_dir.path().to_str().unwrap().to_string();
    }
    
    // Test repository analysis
    {
        let mut app = app.lock().unwrap();
        assert_eq!(app.commit_count, 0);
        assert!(app.top_contributors.is_empty());
        
        // Analyze repository
        let result = gitstats::analysis::analyze_repo_async(
            app.repo_path.clone(),
            "main".to_string(),
            "All".to_string()
        ).await.unwrap();
        
        app.update_with_result(result);
        
        // Verify analysis results
        assert!(app.commit_count > 0);
        assert!(!app.top_contributors.is_empty());
        assert!(app.total_lines_added > 0);
        assert_eq!(app.selected_contributor, "All");
    }
    
    // Test branch selection
    {
        let mut app = app.lock().unwrap();
        assert!(!app.available_branches.is_empty());
        let original_branch = app.selected_branch.clone();
        
        // Switch branch
        if let Some(branch) = app.available_branches.get(0) {
            app.selected_branch = branch.clone();
            assert_ne!(app.selected_branch, original_branch);
        }
    }
    
    // Test plot generation
    {
        let mut app = app.lock().unwrap();
        app.plot_path = temp_dir.path().join("test_plot.png").to_str().unwrap().to_string();
        
        // Test different metrics
        for metric in &["Commits", "Code Changes", "Code Frequency"] {
            app.current_metric = metric.to_string();
            assert!(gitstats::plotting::generate_plot(&app).is_ok());
            assert!(fs::metadata(&app.plot_path).is_ok());
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
                contributor
            ).await.unwrap();
            
            app.update_with_result(result);
            assert!(app.commit_count > 0);
        }
    }
    
    // Test caching
    {
        let app = app.lock().unwrap(); // Removed mut as it's not needed
        let cache_key = gitstats::types::CacheKey {
            branch: app.selected_branch.clone(),
            contributor: app.selected_contributor.clone(),
        };
        
        assert!(app.get_cached_result(&cache_key.branch, &cache_key.contributor).is_some());
    }
}

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
            "All".to_string()
        ).await;
        
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
            "All".to_string()
        ).await;
        
        // Should fall back to HEAD
        assert!(result.is_ok());
    }
} 