use egui::{ComboBox, Context};
use image::ImageReader;
use std::sync::{Arc, Mutex};

use super::App;
use crate::analysis::analyze_repo_async;

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
                        let mut app = app_clone.lock().unwrap();
                        app.update_with_result(result);
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
                let mut app = app_clone.lock().unwrap();
                app.is_analyzing = false;
            });
        }

        if app.is_analyzing {
            ui.label("Analyzing... Please wait.");
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
        if let Err(e) = crate::plotting::generate_plot(app) {
            eprintln!("Plotting error: {}", e);
        } else {
            load_plot_texture(app, ctx);
        }
        app.update_needed = false;
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

        tokio::spawn(async move {
            match analyze_repo_async(repo_path, selected_branch, selected_contributor).await {
                Ok(result) => {
                    let mut app = app_arc.lock().unwrap();
                    app.update_with_result(result);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            }
            let mut app = app_arc.lock().unwrap();
            app.is_analyzing = false;
        });
    }
}

fn load_plot_texture(app: &mut App, ctx: &Context) {
    if let Ok(image) = ImageReader::open(&app.plot_path).and_then(|reader| {
        reader
            .decode()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }) {
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.to_rgba8();
        let pixels = pixels.as_flat_samples();
        let texture = ctx.load_texture(
            "plot_texture",
            egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()),
            egui::TextureOptions::LINEAR,
        );
        app.plot_texture = Some(texture);
    } else {
        eprintln!("Failed to load plot image");
    }
}
