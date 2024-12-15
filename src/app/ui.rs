use egui::{ComboBox, Context};
use image::ImageReader;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;

use super::App;
use crate::analysis::analyze_repo_async;
use crate::plotting::chart::generate_plot_async;

// Progress tracking for long operations
lazy_static! {
    static ref ANALYSIS_PROGRESS: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    static ref PLOTTING_IN_PROGRESS: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

/// Draw the main application UI
pub fn draw_ui(app: &mut App, ctx: &Context, app_arc: Arc<Mutex<App>>) {
    egui::SidePanel::left("side_panel").show(ctx, |ui| {
        ui.heading("Analysis Options");
        ui.separator();

        // Branch selection
        if !app.available_branches.is_empty() {
            ui.label("Branch:");
            let prev_branch = app.selected_branch.clone();
            ComboBox::new("branch_selector", "")
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

        // Contributor selection
        if !app.all_contributors.is_empty() {
            ui.label("Contributor:");
            let mut contributors: Vec<String> = app
                .all_contributors
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            contributors.sort();
            contributors.insert(0, "All".to_string());

            let prev_contributor = app.selected_contributor.clone();
            ComboBox::new("contributor_selector", "")
                .selected_text(&app.selected_contributor)
                .show_ui(ui, |ui| {
                    for contributor in &contributors {
                        ui.selectable_value(
                            &mut app.selected_contributor,
                            contributor.clone(),
                            contributor,
                        );
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
        if ui.button("Top Contributors").clicked() {
            app.current_metric = "Top Contributors".to_string();
            app.update_needed = true;
        }

        ui.separator();
        ui.checkbox(&mut app.use_log_scale, "Log Scale");
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Git Statistics");
        ui.separator();

        ui.label("Enter the path to a Git repository:");
        ui.text_edit_singleline(&mut app.repo_path);

        if ui.button("Analyze").clicked() && !app.is_analyzing {
            let repo_path = app.repo_path.clone();
            let app_clone = app_arc.clone();
            let selected_branch = app.selected_branch.clone();
            let selected_contributor = app.selected_contributor.clone();
            app.is_analyzing = true;

            tokio::spawn(async move {
                match analyze_repo_async(repo_path, selected_branch, selected_contributor).await {
                    Ok(result) => {
                        if let Ok(mut app) = app_clone.lock() {
                            app.update_with_result(result);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
                if let Ok(mut app) = app_clone.lock() {
                    app.is_analyzing = false;
                }
            });
        }

        // Show progress indicators
        if app.is_analyzing {
            let progress = ANALYSIS_PROGRESS.load(Ordering::Relaxed);
            ui.label("Analyzing... Please wait.");
            ui.add(egui::ProgressBar::new(progress as f32 / 100.0));
            ui.spinner();
        }

        ui.separator();
        ui.label(format!("Total commits: {}", app.commit_count));
        ui.label(format!("Total lines added: {}", app.total_lines_added));
        ui.label(format!("Total lines deleted: {}", app.total_lines_deleted));
        ui.label(format!(
            "Average commit size: {:.2}",
            app.average_commit_size
        ));

        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| match app.current_metric.as_str() {
            "Commit Frequency" => {
                ui.label("Commit Frequency:");
                if let Some(texture) = &app.plot_texture {
                    ui.image(texture);
                }
            }
            "Top Contributors" => {
                ui.label("Top Contributors:");
                for (author, count) in &app.top_contributors {
                    ui.label(format!("{}: {}", author, count));
                }
            }
            _ => {
                if let Some(texture) = &app.plot_texture {
                    ui.image(texture);
                }
            }
        });
    });

    // Update plot if needed
    if app.update_needed {
        let plot_path = app.plot_path.clone();
        let app_clone = app.clone();
        let app_arc = app_arc.clone();
        let ctx = ctx.clone();

        // Only start plotting if not already in progress
        if !PLOTTING_IN_PROGRESS.load(Ordering::Relaxed) {
            PLOTTING_IN_PROGRESS.store(true, Ordering::Relaxed);
            
            tokio::spawn(async move {
                // Use a semaphore to limit concurrent plotting operations
                static PLOT_SEMAPHORE: tokio::sync::OnceCell<tokio::sync::Semaphore> = tokio::sync::OnceCell::const_new();
                let semaphore = PLOT_SEMAPHORE.get_or_init(|| async {
                    tokio::sync::Semaphore::new(1) // Only allow one plotting operation at a time
                }).await;
                
                let _permit = match semaphore.acquire().await {
                    Ok(permit) => permit,
                    Err(e) => {
                        eprintln!("Failed to acquire plotting semaphore: {}", e);
                        PLOTTING_IN_PROGRESS.store(false, Ordering::Relaxed);
                        return;
                    }
                };

                // Generate the plot asynchronously
                let plot_result = generate_plot_async(app_clone).await;
                match plot_result {
                    Ok(plot_data) => {
                        // Save the plot to disk
                        if let Err(e) = tokio::fs::write(&plot_path, &plot_data).await {
                            eprintln!("Failed to save plot: {}", e);
                            PLOTTING_IN_PROGRESS.store(false, Ordering::Relaxed);
                            return;
                        }

                        if let Some(image) = load_plot_texture_async(plot_path).await {
                            let texture = ctx.load_texture(
                                "plot_texture",
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            if let Ok(mut app) = app_arc.lock() {
                                app.plot_texture = Some(texture);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Plotting error: {}", e);
                    }
                }

                PLOTTING_IN_PROGRESS.store(false, Ordering::Relaxed);
            });
        }
        
        app.update_needed = false;
    }
}

/// Handle selection changes asynchronously
async fn handle_selection_change_async(
    repo_path: String,
    selected_branch: String,
    selected_contributor: String,
    app_arc: Arc<Mutex<App>>,
    mut cancel_token: tokio::sync::watch::Receiver<bool>,
) {
    ANALYSIS_PROGRESS.store(0, Ordering::Relaxed);

    let app_arc_clone = app_arc.clone();
    let analysis_future = async move {
        match analyze_repo_async(repo_path, selected_branch, selected_contributor).await {
            Ok(result) => {
                if let Ok(mut app) = app_arc_clone.lock() {
                    app.update_with_result(result);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
        ANALYSIS_PROGRESS.store(100, Ordering::Relaxed);
        if let Ok(mut app) = app_arc_clone.lock() {
            app.is_analyzing = false;
        }
    };

    tokio::select! {
        _ = cancel_token.changed() => {
            // Update app state on cancellation
            if let Ok(mut app) = app_arc.lock() {
                app.is_analyzing = false;
            }
            ANALYSIS_PROGRESS.store(0, Ordering::Relaxed);
        }
        _ = analysis_future => {
            // Analysis completed normally
        }
    }
}

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

        // Create cancellation token
        let (sender, receiver) = tokio::sync::watch::channel(false);
        app.cancel_sender = Some(sender);
        
        tokio::spawn(handle_selection_change_async(
            repo_path,
            selected_branch,
            selected_contributor,
            app_arc,
            receiver,
        ));
    }
}

async fn load_plot_texture_async(path: String) -> Option<egui::ColorImage> {
    tokio::task::spawn_blocking(move || {
        ImageReader::open(&path)
            .and_then(|reader| {
                reader
                    .decode()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            })
            .ok()
            .map(|image| {
                let size = [image.width() as usize, image.height() as usize];
                let pixels = image.to_rgba8();
                let pixels = pixels.as_flat_samples();
                egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice())
            })
    })
    .await
    .ok()
    .flatten()
}
