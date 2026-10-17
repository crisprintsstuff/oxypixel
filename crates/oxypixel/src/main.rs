//! OxyPixel, an original pixel-art editor.
//!
//! Release builds on Windows use the GUI subsystem so opening the app does not
//! flash a console. Headless export commands still return a process exit code.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app;
mod canvas;
mod cli;
mod icon;
mod native;
mod shortcuts;
mod theme;

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::dispatch(&args) {
        std::process::exit(code);
    }
    let open = args.first().map(PathBuf::from);
    if let Err(err) = launch(open) {
        native::alert(&err.to_string());
        std::process::exit(1);
    }
}

fn launch(open: Option<PathBuf>) -> eframe::Result<()> {
    let icon = icon::icon_data();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([880.0, 560.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "OxyPixel",
        options,
        Box::new(move |cc| Ok(Box::new(app::OxyApp::new(cc, open)))),
    )
}
