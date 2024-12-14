use eframe::App as EApp;
use egui::TextureHandle;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::types::{AnalysisResult, CacheKey};

/// Main application state
#[derive(Clone)]
pub struct App {
    pub repo_path: String,
    pub commit_count: usize,
    pub total_lines_added: usize,
    pub total_lines_deleted: usize,
    pub top_contributors: Vec<(String, usize)>,
    pub all_contributors: Vec<(String, usize)>,
    pub commit_activity: Vec<(String, usize, usize)>,
    pub plot_path: String,
    pub plot_texture: Option<TextureHandle>,
    pub current_metric: String,
    pub average_commit_size: f64,
    pub commit_frequency: HashMap<String, usize>,
    pub top_contributors_by_lines: Vec<(String, usize)>,
    pub update_needed: bool,
    pub is_analyzing: bool,
    pub use_log_scale: bool,
    pub selected_branch: String,
    pub selected_contributor: String,
    pub available_branches: Vec<String>,
    pub analysis_cache: HashMap<CacheKey, AnalysisResult>,
}

impl App {
    /// Update the app state with new analysis results
    pub fn update_with_result(&mut self, result: AnalysisResult) {
        // Store all contributors if this is the first analysis or if viewing all contributors
        if self.all_contributors.is_empty() || self.selected_contributor == "All" {
            self.all_contributors = result.top_contributors.clone();
        }

        // Update available branches
        if self.available_branches.is_empty() {
            self.available_branches = result.available_branches.clone();
            // Set default branch if not already set
            if self.selected_branch.is_empty() {
                self.selected_branch = self
                    .available_branches
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "main".to_string());
            }
        }

        // Cache the result using both branch and contributor
        let cache_key = CacheKey {
            branch: self.selected_branch.clone(),
            contributor: self.selected_contributor.clone(),
        };
        self.analysis_cache.insert(cache_key, result.clone());

        self.commit_count = result.commit_count;
        self.total_lines_added = result.total_lines_added;
        self.total_lines_deleted = result.total_lines_deleted;
        self.top_contributors = result.top_contributors;
        self.commit_activity = result.commit_activity;
        self.average_commit_size = result.average_commit_size;
        self.commit_frequency = result.commit_frequency;
        self.top_contributors_by_lines = result.top_contributors_by_lines;
        self.update_needed = true;
    }

    /// Get a cached result for the given branch and contributor
    pub fn get_cached_result(&self, branch: &str, contributor: &str) -> Option<AnalysisResult> {
        let cache_key = CacheKey {
            branch: branch.to_string(),
            contributor: contributor.to_string(),
        };
        self.analysis_cache.get(&cache_key).cloned()
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            repo_path: String::new(),
            commit_count: 0,
            total_lines_added: 0,
            total_lines_deleted: 0,
            top_contributors: Vec::new(),
            all_contributors: Vec::new(),
            commit_activity: Vec::new(),
            plot_path: "commit_activity.png".to_string(),
            plot_texture: None,
            current_metric: "Commits".to_string(),
            average_commit_size: 0.0,
            commit_frequency: HashMap::new(),
            top_contributors_by_lines: Vec::new(),
            update_needed: false,
            is_analyzing: false,
            use_log_scale: false,
            selected_branch: "main".to_string(),
            selected_contributor: "All".to_string(),
            available_branches: Vec::new(),
            analysis_cache: HashMap::new(),
        }
    }
}

/// Thread-safe wrapper around App for use with eframe
pub struct AppWrapper {
    pub app: Arc<Mutex<App>>,
}

impl EApp for AppWrapper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut app = self.app.lock().unwrap();
        super::ui::draw_ui(&mut app, ctx, Arc::clone(&self.app));
    }
}
