use eframe::{egui, App};
use egui::TextureHandle;
use git2::{Repository, Error};
use plotters::prelude::*;
use plotters::coord::types::RangedCoordf64;
use plotters::style::text_anchor::{Pos, HPos, VPos};
use std::collections::HashMap;
use image::ImageReader;
use tokio::runtime::Runtime;
use std::sync::{Arc, Mutex};

fn main() {
    // Initialize the Tokio runtime
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Initialize the GUI application with larger window size
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0]),
            ..Default::default()
        };
        if let Err(e) = eframe::run_native(
            "Git Statistics",
            options,
            Box::new(|_cc| {
                let app: Arc<Mutex<MyApp>> = Arc::new(Mutex::new(MyApp::default()));
                Ok(Box::new(AppWrapper { app }))
            }),
        ) {
            eprintln!("Error running application: {}", e);
        }
    });
}

struct AppWrapper {
    app: Arc<Mutex<MyApp>>,
}

impl App for AppWrapper {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut app = self.app.lock().unwrap();
        app.update(ctx, Arc::clone(&self.app));
    }
}

#[derive(Clone)]
struct MyApp {
    repo_path: String,
    commit_count: usize,
    total_lines_added: usize,
    total_lines_deleted: usize,
    top_contributors: Vec<(String, usize)>,
    all_contributors: Vec<(String, usize)>,
    commit_activity: Vec<(String, usize, usize)>,
    plot_path: String,
    plot_texture: Option<TextureHandle>,
    current_metric: String,
    average_commit_size: f64,
    commit_frequency: HashMap<String, usize>,
    top_contributors_by_lines: Vec<(String, usize)>,
    update_needed: bool,
    is_analyzing: bool,
    use_log_scale: bool,
    selected_branch: String,
    selected_contributor: String,
    available_branches: Vec<String>,
    analysis_cache: HashMap<CacheKey, AnalysisResult>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    branch: String,
    contributor: String,
}

impl Default for MyApp {
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

impl MyApp {
    fn update(&mut self, ctx: &egui::Context, app: Arc<Mutex<MyApp>>) {
        if self.update_needed {
            if let Err(e) = self.generate_plot() {
                eprintln!("Plotting error: {}", e);
            } else {
                self.load_plot_texture(ctx);
            }
            self.update_needed = false;
        }

        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("Analysis Options");
            ui.separator();

            // Branch selection
            if !self.available_branches.is_empty() {
                ui.label("Branch:");
                let prev_branch = self.selected_branch.clone();
                egui::ComboBox::new("branch_selector", "")
                    .selected_text(&self.selected_branch)
                    .show_ui(ui, |ui| {
                        for branch in &self.available_branches {
                            ui.selectable_value(&mut self.selected_branch, branch.clone(), branch);
                        }
                    });

                // If branch changed, check cache first
                if prev_branch != self.selected_branch {
                    if let Some(cached_result) = self.get_cached_result(&self.selected_branch, &self.selected_contributor) {
                        // Use cached result
                        self.update_with_result(cached_result);
                    } else {
                        // No cache, perform analysis
                        let repo_path = self.repo_path.clone();
                        let app_clone = Arc::clone(&app);
                        let selected_branch = self.selected_branch.clone();
                        let selected_contributor = self.selected_contributor.clone();
                        self.is_analyzing = true;

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
                }
            }

            // Contributor selection
            if !self.all_contributors.is_empty() {
                ui.label("Contributor:");
                let mut contributors: Vec<String> = self.all_contributors
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect();
                contributors.sort();
                contributors.insert(0, "All".to_string());
                
                let prev_contributor = self.selected_contributor.clone();
                egui::ComboBox::new("contributor_selector", "")
                    .selected_text(&self.selected_contributor)
                    .show_ui(ui, |ui| {
                        for contributor in &contributors {
                            ui.selectable_value(&mut self.selected_contributor, contributor.clone(), contributor);
                        }
                    });
                
                // If contributor changed, check cache first
                if prev_contributor != self.selected_contributor {
                    if let Some(cached_result) = self.get_cached_result(&self.selected_branch, &self.selected_contributor) {
                        // Use cached result
                        self.update_with_result(cached_result);
                    } else {
                        // No cache, perform analysis
                        let repo_path = self.repo_path.clone();
                        let app_clone = Arc::clone(&app);
                        let selected_branch = self.selected_branch.clone();
                        let selected_contributor = self.selected_contributor.clone();
                        self.is_analyzing = true;

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
                }
            }

            ui.separator();

            if ui.button("Commits").clicked() {
                self.current_metric = "Commits".to_string();
                self.update_needed = true;
            }
            if ui.button("Code Changes").clicked() {
                self.current_metric = "Code Changes".to_string();
                self.update_needed = true;
            }
            if ui.button("Code Frequency").clicked() {
                self.current_metric = "Code Frequency".to_string();
                self.update_needed = true;
            }
            if ui.button("Top Contributors").clicked() {
                self.current_metric = "Top Contributors".to_string();
                self.update_needed = true;
            }

            ui.separator();
            ui.checkbox(&mut self.use_log_scale, "Log Scale");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Git Statistics");
            ui.separator();

            ui.label("Enter the path to a Git repository:");
            ui.text_edit_singleline(&mut self.repo_path);

            if ui.button("Analyze").clicked() && !self.is_analyzing {
                let repo_path = self.repo_path.clone();
                let app_clone = Arc::clone(&app);
                let selected_branch = self.selected_branch.clone();
                let selected_contributor = self.selected_contributor.clone();
                self.is_analyzing = true;

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

            if self.is_analyzing {
                ui.label("Analyzing... Please wait.");
                ui.spinner();
            }

            ui.separator();
            ui.label(format!("Total commits: {}", self.commit_count));
            ui.label(format!("Total lines added: {}", self.total_lines_added));
            ui.label(format!("Total lines deleted: {}", self.total_lines_deleted));
            ui.label(format!("Average commit size: {:.2}", self.average_commit_size));

            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.current_metric.as_str() {
                    "Commit Frequency" => {
                        ui.label("Commit Frequency:");
                        if let Some(texture) = &self.plot_texture {
                            ui.image(texture);
                        }
                    }
                    "Top Contributors" => {
                        ui.label("Top Contributors:");
                        for (author, count) in &self.top_contributors {
                            ui.label(format!("{}: {}", author, count));
                        }
                    }
                    _ => {
                        if let Some(texture) = &self.plot_texture {
                            ui.image(texture);
                        }
                    }
                }
            });
        });
    }

    fn update_with_result(&mut self, result: AnalysisResult) {
        // Store all contributors if this is the first analysis or if viewing all contributors
        if self.all_contributors.is_empty() || self.selected_contributor == "All" {
            self.all_contributors = result.top_contributors.clone();
        }
        
        // Update available branches
        if self.available_branches.is_empty() {
            self.available_branches = result.available_branches.clone();
            // Set default branch if not already set
            if self.selected_branch.is_empty() {
                self.selected_branch = self.available_branches.first()
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

    fn calculate_adaptive_range(&self, values: &[f64]) -> (f64, f64) {
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        if sorted.is_empty() {
            return (0.0, 1.0);
        }

        // Remove extreme outliers (values beyond 95th percentile)
        let p95_idx = ((sorted.len() as f64 * 0.95) as usize).max(1).min(sorted.len() - 1);
        let normal_max = sorted[p95_idx];
        let absolute_max = sorted[sorted.len() - 1];
        
        // Use the 95th percentile for the main scale, but ensure we can still see the peaks
        let display_max = if absolute_max > normal_max * 2.0 {
            normal_max * 1.2 // Main scale shows normal range
        } else {
            absolute_max * 1.1 // Show everything if no extreme outliers
        };
        
        (0.0, display_max)
    }

    fn draw_grid(&self, chart_builder: &mut ChartContext<BitMapBackend, Cartesian2d<RangedCoordf64, RangedCoordf64>>, max_val: f64) -> Result<(), Box<dyn std::error::Error>> {
        let grid_style = ShapeStyle::from(&WHITE.mix(0.15)).stroke_width(1);
        let major_grid_style = ShapeStyle::from(&WHITE.mix(0.25)).stroke_width(2);

        // Calculate nice grid intervals
        let y_interval = if max_val > 1_000_000.0 { 100_000.0 }
            else if max_val > 100_000.0 { 10_000.0 }
            else if max_val > 10_000.0 { 1_000.0 }
            else if max_val > 1_000.0 { 100.0 }
            else if max_val > 100.0 { 10.0 }
            else { 1.0 };

        // Draw both major and minor grid lines
        let steps = (max_val / y_interval).ceil() as i32;
        for i in 0..=steps {
            let y = i as f64 * y_interval;
            if y > max_val {
                break;
            }
            let style = if i % 5 == 0 { 
                major_grid_style.clone()
            } else { 
                grid_style.clone()
            };
            chart_builder.draw_series(std::iter::once(PathElement::new(
                vec![(0.0, y), (self.commit_activity.len() as f64, y)],
                style,
            )))?;
        }

        // Draw zero line with higher opacity
        let zero_line_style = ShapeStyle::from(&WHITE.mix(0.3)).stroke_width(2);
        chart_builder.draw_series(std::iter::once(PathElement::new(
            vec![(0.0, 0.0), (self.commit_activity.len() as f64, 0.0)],
            zero_line_style,
        )))?;

        Ok(())
    }

    fn generate_plot(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create the drawing area
        let root_area = BitMapBackend::new(&self.plot_path, (640, 480))
            .into_drawing_area();
        root_area.fill(&BLACK.mix(0.95))?;

        // Get aggregated data
        let plot_data = self.smart_aggregate_data();

        // Calculate range based on data type and adaptive scaling
        let (min_val, max_val) = match self.current_metric.as_str() {
            "Commits" => {
                let commit_values: Vec<f64> = plot_data.iter()
                    .map(|(date, _, _)| {
                        let count = plot_data.iter()
                            .filter(|(d, _, _)| d == date)
                            .count() as f64;
                        count
                    })
                    .collect();
                self.calculate_adaptive_range(&commit_values)
            }
            "Code Changes" | "Code Frequency" => {
                let added_values: Vec<f64> = plot_data.iter()
                    .map(|(_, added, _)| *added as f64)
                    .collect();
                let deleted_values: Vec<f64> = plot_data.iter()
                    .map(|(_, _, deleted)| *deleted as f64)
                    .collect();
                let (_, max_added) = self.calculate_adaptive_range(&added_values);
                let (_, max_deleted) = self.calculate_adaptive_range(&deleted_values);
                let abs_max = max_added.max(max_deleted);
                (-abs_max, abs_max)
            }
            _ => (0.0, 1.0),
        };

        // Get date range for x-axis
        let dates: Vec<String> = plot_data.iter()
            .map(|(date, _, _)| date.clone())
            .collect();

        // Build the chart with improved styling
        let mut chart_builder = ChartBuilder::on(&root_area)
            .caption(
                format!("{} Over Time", self.current_metric),
                ("sans-serif", 30).into_font().color(&WHITE.mix(0.8))
            )
            .margin(10)
            .set_all_label_area_size(50)
            .build_cartesian_2d(
                0f64..(plot_data.len() as f64),
                if self.use_log_scale { 1.0..max_val } else { min_val..max_val }
            )?;

        // Configure mesh with improved styling
        let mut mesh = chart_builder.configure_mesh();
        
        // Store the dates in a longer-lived variable
        let dates_clone = dates.clone();
        let x_label_formatter = move |x: &f64| {
            let idx = *x as usize;
            if idx < dates_clone.len() {
                // Show fewer labels to prevent overlap
                if idx == 0 || idx == dates_clone.len() - 1 || 
                   (idx % (dates_clone.len() / 4).max(1) == 0 && idx > 0 && idx < dates_clone.len() - 1) {
                    dates_clone[idx].clone()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        };

        mesh.light_line_style(&TRANSPARENT)
            .bold_line_style(&WHITE.mix(0.2))
            .axis_style(&WHITE.mix(0.8))
            .y_desc(&self.current_metric)
            .label_style(("sans-serif", 15).into_font().color(&WHITE.mix(0.8)))
            .x_label_formatter(&x_label_formatter)
            // Rotate x labels for better readability
            .x_label_style(("sans-serif", 15)
                .into_font()
                .color(&WHITE.mix(0.8))
                .transform(FontTransform::Rotate90)
                .pos(Pos::new(HPos::Right, VPos::Center)));

        if self.use_log_scale {
            mesh.y_label_formatter(&|y| format!("{:.1e}", y));
        } else {
            // Use K/M formatting for large numbers
            mesh.y_label_formatter(&|y| {
                if y.abs() >= 1_000_000.0 {
                    format!("{:.1}M", y / 1_000_000.0)
                } else if y.abs() >= 1_000.0 {
                    format!("{:.1}K", y / 1_000.0)
                } else {
                    format!("{:.0}", y)
                }
            });
        }

        mesh.draw()?;

        // Draw appropriate grid based on the metric
        self.draw_grid(&mut chart_builder, max_val)?;

        match self.current_metric.as_str() {
            "Code Changes" => {
                // Smooth the data using moving average
                let window_size = if plot_data.len() < 1000 { 3 } else { 2 };
                let mut smoothed_additions: Vec<(f64, f64)> = Vec::new();
                let mut smoothed_deletions: Vec<(f64, f64)> = Vec::new();

                for i in 0..plot_data.len() {
                    let start = i.saturating_sub(window_size / 2);
                    let end = (i + window_size / 2 + 1).min(plot_data.len());
                    let count = end - start;

                    let avg_added = plot_data[start..end]
                        .iter()
                        .map(|(_, added, _)| *added as f64)
                        .sum::<f64>() / count as f64;

                    let avg_deleted = plot_data[start..end]
                        .iter()
                        .map(|(_, _, deleted)| *deleted as f64)
                        .sum::<f64>() / count as f64;

                    smoothed_additions.push((i as f64, avg_added));
                    smoothed_deletions.push((i as f64, -avg_deleted));
                }

                // Draw additions line
                chart_builder.draw_series(LineSeries::new(
                    smoothed_additions,
                    &GREEN.mix(0.8),
                ))?.label("Additions")
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &GREEN.mix(0.8)));

                // Draw deletions line
                chart_builder.draw_series(LineSeries::new(
                    smoothed_deletions,
                    &RED.mix(0.8),
                ))?.label("Deletions")
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED.mix(0.8)));
            }
            "Commits" => {
                // Calculate commit counts
                let mut commit_counts = HashMap::new();
                for (date, _, _) in &plot_data {
                    *commit_counts.entry(date).or_insert(0) += 1;
                }

                // Create raw data points
                let raw_data: Vec<(f64, f64)> = plot_data.iter()
                    .enumerate()
                    .map(|(i, (date, _, _))| {
                        let count = *commit_counts.get(date).unwrap_or(&0);
                        (i as f64, count as f64)
                    })
                    .collect();

                // Smooth the data using moving average
                let window_size = 5; // Larger window for commits to get smoother curve
                let mut smoothed_data: Vec<(f64, f64)> = Vec::new();

                for i in 0..raw_data.len() {
                    let start = i.saturating_sub(window_size / 2);
                    let end = (i + window_size / 2 + 1).min(raw_data.len());
                    let count = end - start;

                    let avg = raw_data[start..end]
                        .iter()
                        .map(|(_, count)| *count)
                        .sum::<f64>() / count as f64;

                    smoothed_data.push((i as f64, avg));
                }

                // Draw a subtle glow effect
                let glow_color = &RGBColor(100, 149, 237).mix(0.3); // Cornflower blue with low opacity
                chart_builder.draw_series(LineSeries::new(
                    smoothed_data.clone(),
                    glow_color.stroke_width(4),
                ))?;

                // Draw the main line with a brighter color
                let line_color = &RGBColor(135, 206, 250); // Light sky blue
                chart_builder.draw_series(LineSeries::new(
                    smoothed_data,
                    line_color.stroke_width(2),
                ))?.label("Commits")
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], line_color));
            }
            "Code Frequency" => {
                let bar_width = 0.8;
                
                // Draw additions (positive bars)
                chart_builder.draw_series(
                    plot_data.iter().enumerate().map(|(i, (_, added, _))| {
                        let x0 = i as f64;
                        let x1 = x0 + bar_width;
                        let y0 = 0.0;
                        let y1 = *added as f64;
                        Rectangle::new([(x0, y0), (x1, y1)], GREEN.mix(0.6).filled())
                    })
                )?.label("Additions")
                    .legend(move |(x, y)| Rectangle::new([(x, y - 5), (x + 20, y + 5)], GREEN.mix(0.6).filled()));

                // Draw deletions (negative bars)
                chart_builder.draw_series(
                    plot_data.iter().enumerate().map(|(i, (_, _, deleted))| {
                        let x0 = i as f64;
                        let x1 = x0 + bar_width;
                        let y0 = 0.0;
                        let y1 = -(*deleted as f64);
                        Rectangle::new([(x0, y0), (x1, y1)], RED.mix(0.6).filled())
                    })
                )?.label("Deletions")
                    .legend(move |(x, y)| Rectangle::new([(x, y - 5), (x + 20, y + 5)], RED.mix(0.6).filled()));
            }
            _ => {}
        }

        // Draw legend with improved styling
        chart_builder.configure_series_labels()
            .background_style(&BLACK.mix(0.8))
            .border_style(&WHITE.mix(0.5))
            .position(SeriesLabelPosition::UpperRight)
            .legend_area_size(30)
            .label_font(("sans-serif", 15).into_font().color(&WHITE.mix(0.8)))
            .draw()?;

        root_area.present()?;
        Ok(())
    }

    fn load_plot_texture(&mut self, ctx: &egui::Context) {
        if let Ok(image) = ImageReader::open(&self.plot_path).and_then(|reader| reader.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))) {
            let size = [image.width() as usize, image.height() as usize];
            let pixels = image.to_rgba8();
            let pixels = pixels.as_flat_samples();
            let texture = ctx.load_texture(
                "plot_texture",
                egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()),
                egui::TextureOptions::LINEAR,
            );
            self.plot_texture = Some(texture);
        } else {
            eprintln!("Failed to load plot image");
        }
    }

    fn smart_aggregate_data(&self) -> Vec<(String, usize, usize)> {
        if self.commit_activity.len() < 1000 {
            return self.commit_activity.clone();
        }

        // Target number of data points for good visualization
        let target_points = 500;
        let window_size = (self.commit_activity.len() as f64 / target_points as f64).ceil() as usize;

        let mut aggregated = Vec::new();
        for chunk in self.commit_activity.chunks(window_size) {
            let date = chunk[0].0.clone(); // Use first date in chunk
            let total_added: usize = chunk.iter().map(|(_, added, _)| *added).sum();
            let total_deleted: usize = chunk.iter().map(|(_, _, deleted)| *deleted).sum();
            aggregated.push((date, total_added, total_deleted));
        }
        aggregated
    }

    fn get_cached_result(&self, branch: &str, contributor: &str) -> Option<AnalysisResult> {
        let cache_key = CacheKey {
            branch: branch.to_string(),
            contributor: contributor.to_string(),
        };
        self.analysis_cache.get(&cache_key).cloned()
    }
}

#[derive(Clone)]
struct AnalysisResult {
    commit_count: usize,
    total_lines_added: usize,
    total_lines_deleted: usize,
    top_contributors: Vec<(String, usize)>,
    commit_activity: Vec<(String, usize, usize)>,
    average_commit_size: f64,
    commit_frequency: HashMap<String, usize>,
    top_contributors_by_lines: Vec<(String, usize)>,
    available_branches: Vec<String>,
}

async fn analyze_repo_async(path: String, branch: String, contributor: String) -> Result<AnalysisResult, Error> {
    tokio::task::spawn_blocking(move || analyze_repo_with_filter(&path, &branch, &contributor))
        .await
        .map_err(|e| Error::from_str(&e.to_string()))?
}

fn get_available_branches(repo: &Repository) -> Result<Vec<String>, Error> {
    let mut branch_names = Vec::new();
    let branches = repo.branches(None)?;
    
    for branch in branches {
        if let Ok((branch, _)) = branch {
            if let Ok(name) = branch.name() {
                if let Some(name) = name {
                    branch_names.push(name.to_string());
                }
            }
        }
    }
    
    // Sort branches alphabetically
    branch_names.sort();
    
    // Ensure "main" or "master" is first if present
    if let Some(main_idx) = branch_names.iter().position(|x| x == "main") {
        branch_names.swap(0, main_idx);
    } else if let Some(master_idx) = branch_names.iter().position(|x| x == "master") {
        branch_names.swap(0, master_idx);
    }
    
    Ok(branch_names)
}

fn analyze_repo_with_filter(path: &str, branch: &str, contributor: &str) -> Result<AnalysisResult, Error> {
    let repo = Repository::open(path)?;
    
    // Get available branches first
    let branches = get_available_branches(&repo)?;
    
    let mut revwalk = repo.revwalk()?;
    
    // Set up branch filtering
    if let Ok(branch_ref) = repo.find_branch(branch, git2::BranchType::Local) {
        if let Some(branch_ref_name) = branch_ref.get().name() {
            revwalk.push_ref(branch_ref_name)?;
        } else {
            revwalk.push_head()?;
        }
    } else {
        revwalk.push_head()?;
    }

    let mut commit_count = 0;
    let mut total_lines_added = 0;
    let mut total_lines_deleted = 0;
    let mut author_commit_count: HashMap<String, usize> = HashMap::new();
    let mut commit_activity: Vec<(String, usize, usize)> = Vec::new();

    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let author = commit.author().name().unwrap_or("Unknown").to_string();

        // Skip if not the selected contributor
        if contributor != "All" && author != contributor {
            continue;
        }

        commit_count += 1;
        *author_commit_count.entry(author).or_insert(0) += 1;

        let time = commit.time().seconds();
        let date = chrono::DateTime::from_timestamp(time, 0)
            .unwrap_or_default()
            .date_naive()
            .to_string();

        let tree = commit.tree()?;
        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        let mut lines_added = 0;
        let mut lines_deleted = 0;

        if let Some(parent_tree) = parent_tree {
            let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
            let stats = diff.stats()?;
            lines_added = stats.insertions();
            lines_deleted = stats.deletions();
            total_lines_added += lines_added;
            total_lines_deleted += lines_deleted;
        }

        commit_activity.push((date, lines_added, lines_deleted));
    }

    let mut top_contributors: Vec<(String, usize)> = author_commit_count.clone().into_iter().collect();
    top_contributors.sort_by(|a, b| b.1.cmp(&a.1));
    top_contributors.truncate(5);

    let average_commit_size = if commit_count > 0 {
        (total_lines_added + total_lines_deleted) as f64 / commit_count as f64
    } else {
        0.0
    };

    let mut commit_frequency: HashMap<String, usize> = HashMap::new();
    for (date, _, _) in &commit_activity {
        let week = date[..7].to_string();
        *commit_frequency.entry(week).or_insert(0) += 1;
    }

    let mut top_contributors_by_lines: Vec<(String, usize)> = author_commit_count.into_iter().collect();
    top_contributors_by_lines.sort_by(|a, b| b.1.cmp(&a.1));
    top_contributors_by_lines.truncate(5);

    Ok(AnalysisResult {
        commit_count,
        total_lines_added,
        total_lines_deleted,
        top_contributors,
        commit_activity,
        average_commit_size,
        commit_frequency,
        top_contributors_by_lines,
        available_branches: branches,
    })
}
