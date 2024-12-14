#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let plot_path = temp_dir.path().join("test_plot.png");
        
        let mut app = App::default();
        app.plot_path = plot_path.to_str().unwrap().to_string();
        app.commit_activity = vec![
            ("2023-01-01".to_string(), 10, 5),
            ("2023-01-02".to_string(), 15, 8),
            ("2023-01-03".to_string(), 20, 10),
        ];
        
        (app, temp_dir)
    }

    #[test]
    fn test_generate_plot() {
        let (app, _temp_dir) = setup_test_app();
        
        // Test different metrics
        for metric in &["Commits", "Code Changes", "Code Frequency"] {
            let mut test_app = app.clone();
            test_app.current_metric = metric.to_string();
            
            assert!(generate_plot(&test_app).is_ok());
            assert!(fs::metadata(&test_app.plot_path).is_ok());
            
            // Check if file is not empty
            let metadata = fs::metadata(&test_app.plot_path).unwrap();
            assert!(metadata.len() > 0);
        }
    }

    #[test]
    fn test_adaptive_range() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 100.0]; // 100.0 is an outlier
        let (min, max) = calculate_adaptive_range(&values);
        
        assert_eq!(min, 0.0);
        assert!(max < 100.0); // Max should be scaled down due to outlier
        assert!(max > 5.0);   // But should still be greater than the normal range
    }

    #[test]
    fn test_empty_plot() {
        let (mut app, _temp_dir) = setup_test_app();
        app.commit_activity.clear();
        
        // Should handle empty data gracefully
        assert!(generate_plot(&app).is_ok());
    }

    #[test]
    fn test_log_scale() {
        let (mut app, _temp_dir) = setup_test_app();
        app.use_log_scale = true;
        
        assert!(generate_plot(&app).is_ok());
    }
} 