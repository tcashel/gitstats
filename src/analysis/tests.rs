#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use git2::{Repository, Signature};

    fn setup_test_repo() -> (TempDir, Repository) {
        let temp_dir = TempDir::new().unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();
        
        // Create an initial commit
        let signature = Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        ).unwrap();

        (temp_dir, repo)
    }

    #[tokio::test]
    async fn test_analyze_empty_repo() {
        let (temp_dir, _repo) = setup_test_repo();
        let result = analyze_repo_async(
            temp_dir.path().to_str().unwrap().to_string(),
            "main".to_string(),
            "All".to_string(),
        ).await;

        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert_eq!(analysis.commit_count, 1);
        assert_eq!(analysis.total_lines_added, 0);
        assert_eq!(analysis.total_lines_deleted, 0);
    }

    #[tokio::test]
    async fn test_analyze_with_commits() {
        let (temp_dir, repo) = setup_test_repo();
        
        // Create a test file with some content
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!\n").unwrap();
        
        // Stage and commit the file
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        
        let signature = Signature::now("Test User", "test@example.com").unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Add test file",
            &tree,
            &[&parent],
        ).unwrap();

        let result = analyze_repo_async(
            temp_dir.path().to_str().unwrap().to_string(),
            "main".to_string(),
            "All".to_string(),
        ).await;

        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert_eq!(analysis.commit_count, 2);
        assert!(analysis.total_lines_added > 0);
    }

    #[test]
    fn test_cache_key() {
        let key1 = CacheKey {
            branch: "main".to_string(),
            contributor: "All".to_string(),
        };
        let key2 = CacheKey {
            branch: "main".to_string(),
            contributor: "All".to_string(),
        };
        let key3 = CacheKey {
            branch: "develop".to_string(),
            contributor: "All".to_string(),
        };

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);

        let mut cache = HashMap::new();
        cache.insert(key1.clone(), AnalysisResult::default());
        assert!(cache.contains_key(&key2));
        assert!(!cache.contains_key(&key3));
    }
} 