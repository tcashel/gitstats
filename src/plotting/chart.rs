use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use plotters::coord::Shift;
use std::collections::HashMap;
use std::sync::Arc;
use std::num::NonZeroUsize;
use lru::LruCache;
use tokio::sync::Mutex as TokioMutex;
use std::time::{Duration, Instant};
use once_cell::sync::Lazy;
use std::error::Error;

use crate::app::App;
use crate::utils::aggregate_data;

type PlotError = Box<dyn Error + Send + Sync>;

// Global plot cache with a 5-minute expiration
static PLOT_CACHE: Lazy<Arc<TokioMutex<LruCache<PlotCacheKey, (Vec<u8>, Instant)>>>> = Lazy::new(|| {
    Arc::new(TokioMutex::new(LruCache::new(NonZeroUsize::new(10).unwrap()))) // Cache up to 10 plots
});

#[derive(Hash, Eq, PartialEq)]
struct PlotCacheKey {
    metric: String,
    use_log_scale: bool,
    data_hash: u64,
}

impl PlotCacheKey {
    fn new(app: &App) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        app.commit_activity.hash(&mut hasher);
        app.top_contributors.hash(&mut hasher);
        
        Self {
            metric: app.current_metric.clone(),
            use_log_scale: app.use_log_scale,
            data_hash: hasher.finish(),
        }
    }
}

// Helper function to wrap errors
fn wrap_err<E>(e: E) -> PlotError 
where 
    E: Into<Box<dyn Error + Send + Sync>>
{
    e.into()
}

/// Generate a plot based on the current app state
pub async fn generate_plot_async(app: App) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let cache_key = PlotCacheKey::new(&app);
    
    // Try to get from cache first
    if let Some((plot_data, timestamp)) = PLOT_CACHE.lock().await.get(&cache_key) {
        if timestamp.elapsed() < Duration::from_secs(300) { // 5 minutes
            return Ok(plot_data.clone());
        }
    }

    // Generate new plot in blocking task
    let plot_data = tokio::task::spawn_blocking(move || {
        let mut buffer = Vec::new();
        {
            let root = BitMapBackend::new(&app.plot_path, (640, 480)).into_drawing_area();
            generate_plot_internal(&app, &root)?;
            root.present()?;

            // Read the file back into buffer
            buffer = std::fs::read(&app.plot_path)?;
            // Clean up the temporary file
            let _ = std::fs::remove_file(&app.plot_path);
        }
        Ok::<_, PlotError>(buffer)
    })
    .await??;

    // Cache the result
    PLOT_CACHE.lock().await.put(cache_key, (plot_data.clone(), Instant::now()));

    Ok(plot_data)
}

/// Internal function to generate the plot
pub fn generate_plot_internal(
    app: &App, 
    root_area: &DrawingArea<BitMapBackend, Shift>
) -> Result<(), PlotError> {
    root_area.fill(&BLACK.mix(0.95)).map_err(wrap_err)?;

    // Get aggregated data
    let plot_data = aggregate_data(&app.commit_activity, 500);

    // Calculate range based on data type and adaptive scaling
    let (min_val, max_val) = match app.current_metric.as_str() {
        "Commits" => {
            let commit_values: Vec<f64> = plot_data
                .iter()
                .map(|(date, _, _)| {
                    let count = plot_data.iter().filter(|(d, _, _)| d == date).count() as f64;
                    count
                })
                .collect();
            calculate_adaptive_range(&commit_values)
        }
        "Code Changes" | "Code Frequency" => {
            let added_values: Vec<f64> = plot_data
                .iter()
                .map(|(_, added, _)| *added as f64)
                .collect();
            let deleted_values: Vec<f64> = plot_data
                .iter()
                .map(|(_, _, deleted)| *deleted as f64)
                .collect();
            let (_, max_added) = calculate_adaptive_range(&added_values);
            let (_, max_deleted) = calculate_adaptive_range(&deleted_values);
            let abs_max = max_added.max(max_deleted);
            (-abs_max, abs_max)
        }
        _ => (0.0, 1.0),
    };

    // Get date range for x-axis
    let dates: Vec<String> = plot_data.iter().map(|(date, _, _)| date.clone()).collect();

    // Build the chart with improved styling
    let mut chart_builder = ChartBuilder::on(&root_area)
        .caption(
            format!("{} Over Time", app.current_metric),
            ("sans-serif", 30).into_font().color(&WHITE.mix(0.8)),
        )
        .margin(10)
        .set_all_label_area_size(50)
        .build_cartesian_2d(
            0f64..(plot_data.len() as f64),
            if app.use_log_scale {
                1.0..max_val
            } else {
                min_val..max_val
            },
        )?;

    // Configure mesh with improved styling
    let mut mesh = chart_builder.configure_mesh();

    // Store the dates in a longer-lived variable
    let dates_clone = dates.clone();
    let x_label_formatter = move |x: &f64| {
        let idx = *x as usize;
        if idx < dates_clone.len() {
            // Show fewer labels to prevent overlap
            if idx == 0
                || idx == dates_clone.len() - 1
                || (idx % (dates_clone.len() / 4).max(1) == 0
                    && idx > 0
                    && idx < dates_clone.len() - 1)
            {
                dates_clone[idx].clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    mesh.light_line_style(TRANSPARENT)
        .bold_line_style(WHITE.mix(0.2))
        .axis_style(WHITE.mix(0.8))
        .y_desc(&app.current_metric)
        .label_style(("sans-serif", 15).into_font().color(&WHITE.mix(0.8)))
        .x_label_formatter(&x_label_formatter)
        // Rotate x labels for better readability
        .x_label_style(
            ("sans-serif", 15)
                .into_font()
                .color(&WHITE.mix(0.8))
                .transform(FontTransform::Rotate90)
                .pos(Pos::new(HPos::Right, VPos::Center)),
        );

    if app.use_log_scale {
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

    // Draw grid and data
    draw_grid(&mut chart_builder, plot_data.len() as f64).map_err(wrap_err)?;
    
    match app.current_metric.as_str() {
        "Commits" => {
            draw_commits(&mut chart_builder, &plot_data).map_err(wrap_err)?;
        }
        "Code Changes" => {
            draw_code_changes(&mut chart_builder, &plot_data).map_err(wrap_err)?;
        }
        "Code Frequency" => {
            draw_code_frequency(&mut chart_builder, &plot_data).map_err(wrap_err)?;
        }
        _ => {}
    }

    Ok(())
}

fn draw_grid(
    chart_builder: &mut ChartContext<BitMapBackend, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    x_max: f64,
) -> Result<(), PlotError> {
    let grid_style = ShapeStyle::from(&WHITE.mix(0.15)).stroke_width(1);
    let major_grid_style = ShapeStyle::from(&WHITE.mix(0.25)).stroke_width(2);

    // Draw horizontal grid lines
    let y_range = chart_builder.y_range();
    let y_min = y_range.start;
    let y_max = y_range.end;
    let y_span = y_max - y_min;

    // Calculate nice grid intervals
    let y_interval = if y_span > 1_000_000.0 {
        100_000.0
    } else if y_span > 100_000.0 {
        10_000.0
    } else if y_span > 10_000.0 {
        1_000.0
    } else if y_span > 1_000.0 {
        100.0
    } else if y_span > 100.0 {
        10.0
    } else {
        1.0
    };

    // Draw both major and minor grid lines
    let steps = (y_span / y_interval).ceil() as i32;
    let y_start = (y_min / y_interval).floor() * y_interval;

    for i in 0..=steps {
        let y = y_start + i as f64 * y_interval;
        if y > y_max {
            break;
        }
        let style = if i % 5 == 0 {
            major_grid_style
        } else {
            grid_style
        };
        chart_builder.draw_series(std::iter::once(PathElement::new(
            vec![(0.0, y), (x_max, y)],
            style,
        )))?;
    }

    // Draw zero line with higher opacity if it's in range
    if y_min <= 0.0 && y_max >= 0.0 {
        let zero_line_style = ShapeStyle::from(&WHITE.mix(0.3)).stroke_width(2);
        chart_builder.draw_series(std::iter::once(PathElement::new(
            vec![(0.0, 0.0), (x_max, 0.0)],
            zero_line_style,
        )))?;
    }

    Ok(())
}

fn draw_code_changes(
    chart_builder: &mut ChartContext<BitMapBackend, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    plot_data: &[(String, usize, usize)],
) -> Result<(), PlotError> {
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
            .sum::<f64>()
            / count as f64;

        let avg_deleted = plot_data[start..end]
            .iter()
            .map(|(_, _, deleted)| *deleted as f64)
            .sum::<f64>()
            / count as f64;

        smoothed_additions.push((i as f64, avg_added));
        smoothed_deletions.push((i as f64, -avg_deleted));
    }

    // Draw additions line
    chart_builder
        .draw_series(LineSeries::new(smoothed_additions, &GREEN.mix(0.8)))?
        .label("Additions")
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN.mix(0.8)));

    // Draw deletions line
    chart_builder
        .draw_series(LineSeries::new(smoothed_deletions, &RED.mix(0.8)))?
        .label("Deletions")
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED.mix(0.8)));

    Ok(())
}

fn draw_commits(
    chart_builder: &mut ChartContext<BitMapBackend, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    plot_data: &[(String, usize, usize)],
) -> Result<(), PlotError> {
    // Calculate commit counts
    let mut commit_counts = HashMap::new();
    for (date, _, _) in plot_data {
        *commit_counts.entry(date).or_insert(0) += 1;
    }

    // Create raw data points
    let raw_data: Vec<(f64, f64)> = plot_data
        .iter()
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
            .sum::<f64>()
            / count as f64;

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
    chart_builder
        .draw_series(LineSeries::new(smoothed_data, line_color.stroke_width(2)))?
        .label("Commits")
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], line_color));

    Ok(())
}

fn draw_code_frequency(
    chart_builder: &mut ChartContext<BitMapBackend, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    plot_data: &[(String, usize, usize)],
) -> Result<(), PlotError> {
    let bar_width = 0.8;

    // Draw additions (positive bars)
    chart_builder
        .draw_series(plot_data.iter().enumerate().map(|(i, (_, added, _))| {
            let x0 = i as f64;
            let x1 = x0 + bar_width;
            let y0 = 0.0;
            let y1 = *added as f64;
            Rectangle::new([(x0, y0), (x1, y1)], GREEN.mix(0.6).filled())
        }))?
        .label("Additions")
        .legend(move |(x, y)| {
            Rectangle::new([(x, y - 5), (x + 20, y + 5)], GREEN.mix(0.6).filled())
        });

    // Draw deletions (negative bars)
    chart_builder
        .draw_series(plot_data.iter().enumerate().map(|(i, (_, _, deleted))| {
            let x0 = i as f64;
            let x1 = x0 + bar_width;
            let y0 = 0.0;
            let y1 = -(*deleted as f64);
            Rectangle::new([(x0, y0), (x1, y1)], RED.mix(0.6).filled())
        }))?
        .label("Deletions")
        .legend(move |(x, y)| Rectangle::new([(x, y - 5), (x + 20, y + 5)], RED.mix(0.6).filled()));

    Ok(())
}

fn calculate_adaptive_range(values: &[f64]) -> (f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if sorted.is_empty() {
        return (0.0, 1.0);
    }

    // Remove extreme outliers (values beyond 95th percentile)
    let p95_idx = ((sorted.len() as f64 * 0.95) as usize)
        .max(1)
        .min(sorted.len() - 1);
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
