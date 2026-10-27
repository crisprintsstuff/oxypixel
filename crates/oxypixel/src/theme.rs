//! Original dark theme. The amber accent is the same hue as the app icon.

use eframe::egui::{Color32, Visuals};

pub const AMBER: Color32 = Color32::from_rgb(232, 168, 56);
pub const TEAL: Color32 = Color32::from_rgb(58, 161, 122);
pub const INK: Color32 = Color32::from_rgb(20, 22, 30);
pub const PANEL: Color32 = Color32::from_rgb(28, 31, 40);
pub const CHECKER_A: Color32 = Color32::from_rgb(46, 49, 61);
pub const CHECKER_B: Color32 = Color32::from_rgb(24, 26, 34);
pub const CHECKER_LIGHT_A: Color32 = Color32::from_rgb(214, 214, 220);
pub const CHECKER_LIGHT_B: Color32 = Color32::from_rgb(250, 250, 252);

pub fn apply(ctx: &eframe::egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = Color32::from_rgb(24, 26, 34);
    visuals.extreme_bg_color = INK;
    visuals.selection.bg_fill = Color32::from_rgb(120, 86, 32);
    visuals.selection.stroke.color = AMBER;
    visuals.hyperlink_color = TEAL;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 41, 54);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(54, 58, 74);
    visuals.widgets.active.bg_fill = Color32::from_rgb(78, 62, 32);
    ctx.set_visuals(visuals);
}
