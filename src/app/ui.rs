/// Module for handling the application's user interface using egui.
/// Provides functions for drawing the UI and handling user interactions.
use egui::Context;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::App;
use crate::analysis::analyze_repo_async;

/// Draw the main application UI
pub fn draw_ui(app: &mut App, ctx: &Context, app_arc: Arc<Mutex<App>>) {
    egui::SidePanel::left("side_panel").show(ctx, |ui| {
        ui.heading("Analysis Options");
        ui.separator();

        // Repository path input
        ui.horizontal(|ui| {
            ui.label("Repository Path:");
            ui.text_edit_singleline(&mut app.repo_path);
        });

        // Branch and contributor selection
        if !app.available_branches.is_empty() {
            ui.label("Branch:");
            let prev_branch = app.selected_branch.clone();
            egui::ComboBox::new("branch_selector", "")
                .selected_text(&app.selected_branch)
                .show_ui(ui, |ui| {
                    for branch in &app.available_branches {
                        ui.selectable_value(&mut app.selected_branch, branch.clone(), branch);
                    }
                });

            // Handle branch change
            if prev_branch != app.selected_branch {
                handle_selection_change(app, app_arc.clone());
            }
        }

        if !app.all_contributors.is_empty() {
            ui.label("Contributor:");
            let prev_contributor = app.selected_contributor.clone();
            egui::ComboBox::new("contributor_selector", "")
                .selected_text(&app.selected_contributor)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.selected_contributor, "All".to_string(), "All");
                    for (author, _) in &app.all_contributors {
                        ui.selectable_value(&mut app.selected_contributor, author.clone(), author);
                    }
                });

            // Handle contributor change
            if prev_contributor != app.selected_contributor {
                handle_selection_change(app, app_arc.clone());
            }
        }

        ui.separator();

        // Metric selection buttons
        if ui.button("Commits").clicked() {
            app.current_metric = "Commits".to_string();
            app.update_needed = true;
        }
        if ui.button("Code Changes").clicked() {
            app.current_metric = "Code Changes".to_string();
            app.update_needed = true;
        }
        if ui.button("Code Frequency").clicked() {
            app.current_metric = "Code Frequency".to_string();
            app.update_needed = true;
        }

        ui.separator();
        ui.checkbox(&mut app.use_log_scale, "Log Scale");

        // Performance metrics
        if let Some(analysis_time) = app.last_analysis_time {
            ui.separator();
            ui.heading("Performance Metrics");
            ui.label(format!("Analysis Time: {:.2}s", analysis_time));
            if let Some(commits_per_sec) = app.commits_per_second {
                ui.label(format!("Commits/sec: {:.1}", commits_per_sec));
            }
            if !app.processing_stats.is_empty() {
                ui.label(&app.processing_stats);
            }
        }
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Git Repository Analysis");

        // Analyze button
        if ui.button("Analyze Repository").clicked() && !app.is_analyzing {
            app.is_analyzing = true;
            app.error_message = None;
            let repo_path = app.repo_path.clone();
            let selected_branch = app.selected_branch.clone();
            let selected_contributor = app.selected_contributor.clone();
            let app_clone = app_arc.clone();

            tokio::spawn(async move {
                let (tx, mut rx) = mpsc::channel(32);
                let analyze_future =
                    analyze_repo_async(repo_path, selected_branch, selected_contributor, Some(tx));

                // Spawn a task to handle progress updates
                let progress_app = app_clone.clone();
                tokio::spawn(async move {
                    while let Some(progress) = rx.recv().await {
                        if let Ok(mut app) = progress_app.lock() {
                            app.update_progress(progress);
                        }
                    }
                });

                // Wait for analysis to complete
                match analyze_future.await {
                    Ok(result) => {
                        if let Ok(mut app) = app_clone.lock() {
                            app.update_with_result(result);
                            app.is_analyzing = false;
                        }
                    }
                    Err(e) => {
                        if let Ok(mut app) = app_clone.lock() {
                            app.error_message = Some(e.to_string());
                            app.is_analyzing = false;
                        }
                    }
                }
            });
        }

        // Show progress if available
        if let Some(progress) = &app.progress {
            ui.add(
                egui::ProgressBar::new(progress.percent_complete() as f32 / 100.0)
                    .text(format!("{:.1}%", progress.percent_complete())),
            );
            ui.label(format!(
                "Processing {} commits ({:.1} commits/sec)",
                progress.processed_commits, progress.commits_per_second
            ));
            ui.label(format!(
                "Estimated time remaining: {:.1} seconds",
                progress.estimated_remaining_time()
            ));
        }

        if app.is_analyzing {
            ui.spinner();
        }

        // Show error if any
        if let Some(error) = &app.error_message {
            ui.colored_label(egui::Color32::RED, error);
        }

        ui.separator();

        // Show results if available
        if let Some(result) = &app.analysis_result {
            ui.heading("Analysis Results");
            ui.label(format!("Total Commits: {}", result.commit_count));
            ui.label(format!(
                "Lines Added/Deleted: +{}/−{}",
                result.total_lines_added, result.total_lines_deleted
            ));
            ui.label(format!(
                "Average Commit Size: {:.1} lines",
                result.average_commit_size
            ));

            ui.heading("Top Contributors");
            for (author, count) in &result.top_contributors {
                ui.label(format!("{}: {} commits", author, count));
            }
        }

        // Show plot
        if let Some(texture) = &app.plot_texture {
            ui.image(texture);
        }

        // Update plot if needed
        if app.update_needed {
            app.update_needed = false;
            let app_clone = app_arc.clone();
            let ctx = ctx.clone();

            // Drop the mutex guard before spawning the async task
            let app_data = app.clone();
            tokio::spawn(async move {
                if let Ok(plot_data) = crate::plotting::generate_plot_async(app_data).await {
                    // The plot data should be in RGBA format, where each pixel is 4 bytes
                    let width = 640; // Fixed width
                    let height = 480; // Fixed height
                    let expected_size = width * height * 4; // 4 bytes per pixel (RGBA)

                    if plot_data.len() == expected_size {
                        let texture = ctx.load_texture(
                            "plot_texture",
                            egui::ColorImage::from_rgba_unmultiplied([width, height], &plot_data),
                            egui::TextureOptions::LINEAR,
                        );

                        // Only lock the mutex when updating the texture
                        if let Ok(mut app) = app_clone.lock() {
                            app.plot_texture = Some(texture);
                        }
                    } else {
                        eprintln!(
                            "Invalid plot data size: got {} bytes, expected {} bytes",
                            plot_data.len(),
                            expected_size
                        );
                    }
                }
            });
        }
    });

    // Request a repaint to keep the UI updating
    ctx.request_repaint();
}

/// Handle changes in branch or contributor selection
/// Updates the analysis results either from cache or by running a new analysis
///
/// # Arguments
/// * `app` - Mutable reference to the application state
/// * `app_arc` - Thread-safe reference to the application state for async operations
fn handle_selection_change(app: &mut App, app_arc: Arc<Mutex<App>>) {
    if let Some(cached_result) =
        app.get_cached_result(&app.selected_branch, &app.selected_contributor)
    {
        // Use cached result
        app.update_with_result(cached_result);
    } else {
        // No cache, perform analysis
        let repo_path = app.repo_path.clone();
        let selected_branch = app.selected_branch.clone();
        let selected_contributor = app.selected_contributor.clone();
        app.is_analyzing = true;

        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel(32);
            let analyze_future =
                analyze_repo_async(repo_path, selected_branch, selected_contributor, Some(tx));

            // Spawn a task to handle progress updates
            let progress_app = app_arc.clone();
            tokio::spawn(async move {
                while let Some(progress) = rx.recv().await {
                    if let Ok(mut app) = progress_app.lock() {
                        app.update_progress(progress);
                    }
                }
            });

            // Wait for analysis to complete
            match analyze_future.await {
                Ok(result) => {
                    if let Ok(mut app) = app_arc.lock() {
                        app.update_with_result(result);
                        app.is_analyzing = false;
                    }
                }
                Err(e) => {
                    if let Ok(mut app) = app_arc.lock() {
                        app.error_message = Some(e.to_string());
                        app.is_analyzing = false;
                    }
                }
            }
        });
    }
}
