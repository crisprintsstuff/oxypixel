//! OxyPixel, an original pixel-art editor.
//!
//! Release builds on Windows use the GUI subsystem so opening the app does not
//! flash a console. Headless export commands still return a process exit code.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app;
mod cache;
mod canvas;
mod cli;
mod clipboard;
mod framestrip;
mod icon;
mod native;
mod packview;
mod preview;
mod recent;
mod recovery;
mod refsview;
mod session;
mod shortcuts;
mod tagsview;
mod theme;
mod toolicons;
mod update;

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::dispatch(&args) {
        std::process::exit(code);
    }
    let open = args.first().map(PathBuf::from);
    let result = match launch(open.clone(), eframe::Renderer::Wgpu) {
        // wgpu found no adapter or could not make a surface. OpenGL may still
        // work on this machine, so try it before giving up.
        Err(err) if falls_back_to_opengl(&err) => {
            eprintln!("oxypixel: wgpu could not start ({err}); trying OpenGL");
            launch(open, eframe::Renderer::Glow)
        }
        other => other,
    };
    if let Err(err) = result {
        native::alert(&err.to_string());
        std::process::exit(1);
    }
}

/// Only a wgpu setup failure is worth a second try with OpenGL.
fn falls_back_to_opengl(err: &eframe::Error) -> bool {
    matches!(err, eframe::Error::Wgpu(_))
}

fn launch(open: Option<PathBuf>, renderer: eframe::Renderer) -> eframe::Result<()> {
    let icon = icon::icon_data();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([880.0, 560.0])
            .with_icon(icon),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "OxyPixel",
        options,
        Box::new(move |cc| Ok(Box::new(app::OxyApp::new(cc, open)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_wgpu_setup_failure_retries_with_opengl() {
        let no_adapter = eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::NoSuitableAdapterFound(
            "no adapter".to_string(),
        ));
        assert!(falls_back_to_opengl(&no_adapter));
        let surface = eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::NoSurfaceFormatsAvailable);
        assert!(falls_back_to_opengl(&surface));
        let app = eframe::Error::AppCreation("the app failed".into());
        assert!(!falls_back_to_opengl(&app));
    }
}
