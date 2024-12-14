//! # Git Statistics Visualization Library
//!
//! `gitstats` is a library for analyzing and visualizing Git repository statistics.
//! It provides functionality to analyze Git repositories and generate interactive
//! visualizations of commit history, code changes, and contributor activity.
//!
//! ## Features
//!
//! - Analyze Git repository commit history
//! - Track code changes over time
//! - Identify top contributors
//! - Generate interactive visualizations
//! - Support for branch-specific analysis
//! - Contributor filtering
//! - Caching of analysis results
//!
//! ## Example
//!
//! ```no_run
//! use gitstats::GitStatsApp;
//! use std::sync::{Arc, Mutex};
//! use eframe::NativeOptions;
//!
//! // Create a new application instance
//! let app = Arc::new(Mutex::new(GitStatsApp::default()));
//! let app_wrapper = gitstats::app::AppWrapper { app };
//!
//! // Run the application with eframe
//! eframe::run_native(
//!     "Git Statistics",
//!     NativeOptions::default(),
//!     Box::new(|_cc| Ok(Box::new(app_wrapper))),
//! ).unwrap();
//! ```

pub mod analysis;
pub mod app;
pub mod plotting;
pub mod types;
pub mod utils;

// Re-export main types for convenience
pub use app::App as GitStatsApp;
pub use types::{AnalysisResult, CacheKey};
