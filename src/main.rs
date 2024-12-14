//! Git Statistics Visualization Tool
//! 
//! A GUI application for analyzing and visualizing Git repository statistics.

use eframe::egui;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

use gitstats::app::{App, AppWrapper};

fn main() {
    // Initialize the Tokio runtime
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Initialize the GUI application with larger window size
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_min_inner_size([800.0, 600.0])
                .with_title("Git Statistics"),
            ..Default::default()
        };
        
        if let Err(e) = eframe::run_native(
            "Git Statistics",
            options,
            Box::new(|cc| {
                // Configure default fonts and style
                let fonts = egui::FontDefinitions::default();
                cc.egui_ctx.set_fonts(fonts);
                
                let app: Arc<Mutex<App>> = Arc::new(Mutex::new(App::default()));
                Ok(Box::new(AppWrapper { app }) as Box<dyn eframe::App>)
            }),
        ) {
            eprintln!("Error running application: {}", e);
        }
    });
}
