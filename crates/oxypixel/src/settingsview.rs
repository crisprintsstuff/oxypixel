//! The Settings window (Edit, then Settings…, or Ctrl+,).
//!
//! It edits a copy of [`Prefs`] and reports what changed; the app applies the
//! change to the open view and saves `view.txt` right away. Autosave and the
//! recent files list live in their own files, so they come back as actions.

use eframe::egui::{self, RichText};
use oxypixel_core::MAX_DIMENSION;

use crate::canvas::{CheckerStyle, ONION_FRAME_CAP};
use crate::session::{Prefs, EXPORT_SCALES, UI_SCALES};

#[derive(Default)]
pub struct SettingsWindow {
    pub open: bool,
}

/// What the app should do after this frame of the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsActions {
    /// The preferences the window shows now. `Some` only when they differ.
    pub prefs: Option<Prefs>,
    /// Turn autosave recovery on or off.
    pub autosave: Option<bool>,
    pub clear_recent: bool,
    pub check_updates: bool,
}

/// What the window needs to know that is not in [`Prefs`].
pub struct SettingsContext {
    pub autosave: bool,
    pub recent_count: usize,
}

const GRID_SPACING: egui::Vec2 = egui::Vec2::new(16.0, 8.0);

impl SettingsWindow {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        prefs: &Prefs,
        state: &SettingsContext,
    ) -> SettingsActions {
        let mut actions = SettingsActions::default();
        if !self.open {
            return actions;
        }
        let mut draft = *prefs;
        let mut autosave = state.autosave;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.screen_rect().center())
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Changes apply right away and are kept for the next launch.")
                        .small()
                        .weak(),
                );
                let max_height = (ctx.screen_rect().height() - 160.0).max(200.0);
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, |ui| {
                    section(ui, "General", |ui| {
                        ui.label("Interface scale");
                        egui::ComboBox::from_id_salt("settings-ui-scale")
                            .selected_text(format!("{}%", draft.ui_scale))
                            .show_ui(ui, |ui| {
                                for scale in UI_SCALES {
                                    ui.selectable_value(&mut draft.ui_scale, scale, format!("{scale}%"));
                                }
                            });
                        ui.end_row();
                        ui.label("Preview panel");
                        ui.checkbox(&mut draft.preview_open, "Show the preview panel");
                        ui.end_row();
                        ui.label("Autosave");
                        ui.checkbox(&mut autosave, "Keep a recovery copy while you work")
                            .on_hover_text(
                                "About once a minute, into the app data folder. The open file is never overwritten.",
                            );
                        ui.end_row();
                        ui.label("Recent files");
                        ui.horizontal(|ui| {
                            ui.label(match state.recent_count {
                                1 => "1 file".to_string(),
                                n => format!("{n} files"),
                            });
                            if ui
                                .add_enabled(state.recent_count > 0, egui::Button::new("Clear"))
                                .clicked()
                            {
                                actions.clear_recent = true;
                            }
                        });
                        ui.end_row();
                    });
                    section(ui, "Canvas", |ui| {
                        ui.label("New canvas size");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut draft.new_width)
                                    .range(1..=MAX_DIMENSION)
                                    .suffix(" px"),
                            );
                            ui.label("×");
                            ui.add(
                                egui::DragValue::new(&mut draft.new_height)
                                    .range(1..=MAX_DIMENSION)
                                    .suffix(" px"),
                            );
                        });
                        ui.end_row();
                        ui.label("Grid");
                        ui.checkbox(&mut draft.grid, "Show the pixel grid");
                        ui.end_row();
                        ui.label("Background");
                        egui::ComboBox::from_id_salt("settings-checker")
                            .selected_text(draft.checker.label())
                            .show_ui(ui, |ui| {
                                for style in CheckerStyle::ALL {
                                    ui.selectable_value(&mut draft.checker, style, style.label());
                                }
                            });
                        ui.end_row();
                    });
                    section(ui, "Onion skin", |ui| {
                        ui.label("Frames before");
                        ui.add(egui::Slider::new(&mut draft.onion_previous, 0..=ONION_FRAME_CAP));
                        ui.end_row();
                        ui.label("Frames after");
                        ui.add(egui::Slider::new(&mut draft.onion_next, 0..=ONION_FRAME_CAP));
                        ui.end_row();
                        ui.label("Opacity");
                        ui.add(egui::Slider::new(&mut draft.onion_opacity, 0..=255));
                        ui.end_row();
                        ui.label("Tint");
                        ui.checkbox(&mut draft.onion_tint, "Earlier frames rose, later frames teal");
                        ui.end_row();
                    });
                    ui.label(
                        RichText::new("Turn onion skin on with the Onion box under the frame strip.")
                            .small()
                            .weak(),
                    );
                    section(ui, "Export", |ui| {
                        ui.label("PNG and spritesheet scale");
                        ui.horizontal(|ui| {
                            for scale in EXPORT_SCALES {
                                ui.selectable_value(&mut draft.export_scale, scale, format!("{scale}×"));
                            }
                        });
                        ui.end_row();
                    });
                    section(ui, "Updates", |ui| {
                        ui.label("Automatic updates");
                        ui.checkbox(&mut draft.auto_update, "Check once a day at launch")
                            .on_hover_text(
                                "A newer release is downloaded and checked against SHA256SUMS.txt in the background. Nothing is installed until you click.",
                            );
                        ui.end_row();
                        ui.label("");
                        if ui.button("Check now").clicked() {
                            actions.check_updates = true;
                        }
                        ui.end_row();
                    });
                    });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Reset to defaults")
                        .on_hover_text("Every setting here goes back to how OxyPixel starts. Autosave turns back on.")
                        .clicked()
                    {
                        draft.reset();
                        autosave = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
        self.open = open && !close;
        if draft != *prefs {
            actions.prefs = Some(draft);
        }
        if autosave != state.autosave {
            actions.autosave = Some(autosave);
        }
        actions
    }
}

/// A heading and a two-column grid of label and control.
fn section(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
    egui::Grid::new(("settings", title))
        .num_columns(2)
        .spacing(GRID_SPACING)
        .min_col_width(150.0)
        .show(ui, rows);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(window: &mut SettingsWindow, prefs: &Prefs) -> SettingsActions {
        let ctx = egui::Context::default();
        let state = SettingsContext {
            autosave: true,
            recent_count: 2,
        };
        let mut actions = SettingsActions::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            actions = window.show(ctx, prefs, &state);
        });
        actions
    }

    #[test]
    fn an_untouched_window_changes_nothing() {
        let mut window = SettingsWindow { open: true };
        let prefs = Prefs {
            grid: false,
            ui_scale: 125,
            ..Prefs::default()
        };
        assert_eq!(run(&mut window, &prefs), SettingsActions::default());
        assert!(window.open);
        let mut closed = SettingsWindow::default();
        assert_eq!(run(&mut closed, &prefs), SettingsActions::default());
        assert!(!closed.open);
    }
}
