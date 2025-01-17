//! # Common Types
//!
//! This module contains the common types used throughout the application for
//! representing Git repository analysis results and caching.

use std::collections::HashMap;

/// A key used for caching analysis results based on branch and contributor.
///
/// This struct is used as a key in the cache to store analysis results for specific
/// combinations of branch and contributor names.
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct CacheKey {
    /// The name of the Git branch
    pub branch: String,
    /// The name of the contributor (or "All" for all contributors)
    pub contributor: String,
}

/// The result of analyzing a Git repository.
///
/// This struct contains all the statistics and metrics collected from analyzing
/// a Git repository, including commit counts, line changes, and contributor information.
#[derive(Clone, Debug, Default)]
pub struct AnalysisResult {
    /// Total number of commits analyzed
    pub commit_count: usize,
    /// Total number of lines added across all commits
    pub total_lines_added: usize,
    /// Total number of lines deleted across all commits
    pub total_lines_deleted: usize,
    /// List of top contributors and their commit counts
    pub top_contributors: Vec<(String, usize)>,
    /// Chronological list of commit activity (date, lines added, lines deleted)
    pub commit_activity: Vec<(String, usize, usize)>,
    /// Average number of lines changed per commit
    pub average_commit_size: f64,
    /// Commit frequency by time period (e.g., by week)
    pub commit_frequency: HashMap<String, usize>,
    /// List of top contributors sorted by lines of code
    pub top_contributors_by_lines: Vec<(String, usize)>,
    /// List of available branches in the repository
    pub available_branches: Vec<String>,
    /// Time taken to analyze the repository (in seconds)
    pub elapsed_time: f64,
    /// Detailed processing statistics
    pub processing_stats: String,
}

/// Progress estimation for long-running operations
#[derive(Debug, Clone)]
pub struct ProgressEstimate {
    pub total_commits: usize,
    pub processed_commits: usize,
    pub estimated_total_time: f64,
    pub elapsed_time: f64,
    pub commits_per_second: f64,
}

impl ProgressEstimate {
    pub fn percent_complete(&self) -> f64 {
        if self.total_commits == 0 {
            0.0
        } else {
            (self.processed_commits as f64 / self.total_commits as f64) * 100.0
        }
    }

    pub fn estimated_remaining_time(&self) -> f64 {
        if self.commits_per_second == 0.0 {
            0.0
        } else {
            let remaining_commits = self.total_commits - self.processed_commits;
            remaining_commits as f64 / self.commits_per_second
        }
    }
}
