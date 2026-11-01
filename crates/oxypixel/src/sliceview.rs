//! File, then Import spritesheet…: cut a sheet into frames or layers.
//!
//! The window shows the sheet with the cells that will be kept outlined, and
//! updates as the cell size, offset, and gap change. Import makes a new
//! document the way Import image as document… does.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, Stroke};
use oxypixel_core::{
    cell_corners, read_sheet, slice_sheet, Document, Sheet, SliceGrid, SliceInto, DEFAULT_FRAME_MS,
    MAX_DIMENSION, MAX_FRAME_MS, MAX_SHEET_PADDING, MIN_FRAME_MS,
};

/// Largest side of the preview texture. A bigger sheet is sampled down.
const MAX_PREVIEW_SIDE: u32 = 2048;
/// Room the preview may take in the window, in points.
const PREVIEW_BOX: egui::Vec2 = egui::vec2(420.0, 280.0);

pub(crate) enum SliceAction {
    Open,
    Cancel,
    Import(Document),
}

pub(crate) struct SliceDialog {
    pub path: PathBuf,
    sheet: Sheet,
    pub grid: SliceGrid,
    pub into: SliceInto,
    pub duration_ms: u32,
    /// The kept cells for `corners_for`, or why the grid is not usable.
    corners: Result<Vec<(u32, u32)>, String>,
    corners_for: SliceGrid,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}

impl SliceDialog {
    pub fn open(path: &Path) -> Result<Self, String> {
        let sheet = read_sheet(path).map_err(|err| err.to_string())?;
        let grid = SliceGrid::guess(sheet.width, sheet.height);
        let corners = cell_corners(&sheet, &grid).map_err(|err| err.to_string());
        Ok(Self {
            path: path.to_path_buf(),
            sheet,
            grid,
            into: SliceInto::Frames,
            duration_ms: DEFAULT_FRAME_MS,
            corners,
            corners_for: grid,
            texture: None,
            error: None,
        })
    }

    /// How many cells the current grid keeps.
    pub fn cell_count(&self) -> usize {
        self.corners.as_ref().map_or(0, Vec::len)
    }

    fn refresh(&mut self) {
        if self.corners_for != self.grid {
            self.corners = cell_corners(&self.sheet, &self.grid).map_err(|err| err.to_string());
            self.corners_for = self.grid;
            self.error = None;
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> SliceAction {
        let mut action = SliceAction::Open;
        let (sw, sh) = (self.sheet.width, self.sheet.height);
        egui::Window::new("Import spritesheet")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{}, {sw}×{sh}. Cells are read row by row; a cell that runs off the edge is left out.",
                    self.path.file_name().unwrap_or_default().to_string_lossy()
                ));
                egui::Grid::new("slice-grid").num_columns(3).show(ui, |ui| {
                    let max_side = MAX_DIMENSION;
                    ui.label("Cell size");
                    ui.add(egui::DragValue::new(&mut self.grid.cell_width).range(1..=max_side).suffix(" px"));
                    ui.add(egui::DragValue::new(&mut self.grid.cell_height).range(1..=max_side).suffix(" px"));
                    ui.end_row();
                    ui.label("Offset");
                    ui.add(egui::DragValue::new(&mut self.grid.offset_x).range(0..=sw.saturating_sub(1)).suffix(" px"))
                        .on_hover_text("Pixels to skip at the left edge.");
                    ui.add(egui::DragValue::new(&mut self.grid.offset_y).range(0..=sh.saturating_sub(1)).suffix(" px"))
                        .on_hover_text("Pixels to skip at the top edge.");
                    ui.end_row();
                    ui.label("Gap");
                    ui.add(egui::DragValue::new(&mut self.grid.gap_x).range(0..=MAX_SHEET_PADDING).suffix(" px"))
                        .on_hover_text("Pixels between columns.");
                    ui.add(egui::DragValue::new(&mut self.grid.gap_y).range(0..=MAX_SHEET_PADDING).suffix(" px"))
                        .on_hover_text("Pixels between rows.");
                    ui.end_row();
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Cells of");
                    for side in [8, 16, 24, 32, 48, 64] {
                        if side <= sw.max(sh) && ui.button(format!("{side}×{side}")).clicked() {
                            self.grid.cell_width = side;
                            self.grid.cell_height = side;
                        }
                    }
                    if ui.button("Whole image").on_hover_text("One cell the size of the image.").clicked() {
                        self.grid = SliceGrid::cells_of(sw.min(MAX_DIMENSION), sh.min(MAX_DIMENSION));
                    }
                });
                ui.checkbox(&mut self.grid.keep_empty, "Keep empty cells")
                    .on_hover_text("Off leaves out cells with no visible pixel.");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.into, SliceInto::Frames, "One frame per cell");
                    ui.radio_value(&mut self.into, SliceInto::Layers, "One layer per cell");
                });
                ui.add_enabled_ui(self.into == SliceInto::Frames, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Frame duration");
                        ui.add(egui::DragValue::new(&mut self.duration_ms).range(MIN_FRAME_MS..=MAX_FRAME_MS).suffix(" ms"));
                    });
                });
                self.refresh();
                match &self.corners {
                    Ok(corners) => {
                        let (cols, rows) = self.grid.columns_rows(sw, sh);
                        ui.label(format!(
                            "{} kept of {cols}×{rows}, each {}×{}.",
                            counted(corners.len(), "cell", "cells"),
                            self.grid.cell_width,
                            self.grid.cell_height
                        ));
                    }
                    Err(err) => {
                        ui.colored_label(Color32::from_rgb(220, 100, 90), err);
                    }
                }
                self.preview(ui);
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::from_rgb(220, 100, 90), error);
                }
                ui.horizontal(|ui| {
                    let ready = self.cell_count() > 0;
                    if ui.add_enabled(ready, egui::Button::new("Import")).clicked()
                        || (ready && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        match slice_sheet(&self.sheet, &self.grid, self.into, self.duration_ms) {
                            Ok(doc) => action = SliceAction::Import(doc),
                            Err(err) => self.error = Some(err.to_string()),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        action = SliceAction::Cancel;
                    }
                });
            });
        action
    }

    /// The sheet, scaled to fit, with each kept cell outlined.
    fn preview(&mut self, ui: &mut egui::Ui) {
        let (sw, sh) = (self.sheet.width, self.sheet.height);
        let texture = self.texture.get_or_insert_with(|| {
            let image = preview_image(&self.sheet);
            ui.ctx()
                .load_texture("slice-sheet", image, egui::TextureOptions::NEAREST)
        });
        let fit = (PREVIEW_BOX.x / sw as f32).min(PREVIEW_BOX.y / sh as f32);
        // Whole-pixel scales when the sheet is small, so pixels stay square.
        let scale = if fit >= 1.0 { fit.floor() } else { fit };
        let size = egui::vec2(sw as f32 * scale, sh as f32 * scale);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(40));
        painter.image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if let Ok(corners) = &self.corners {
            let cell = egui::vec2(
                self.grid.cell_width as f32 * scale,
                self.grid.cell_height as f32 * scale,
            );
            let stroke = Stroke::new(1.0_f32, Color32::from_rgb(240, 180, 60));
            for &(x, y) in corners {
                let min = rect.min + egui::vec2(x as f32 * scale, y as f32 * scale);
                painter.rect_stroke(
                    egui::Rect::from_min_size(min, cell),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

/// The sheet as a texture no larger than `MAX_PREVIEW_SIDE` on a side.
fn preview_image(sheet: &Sheet) -> egui::ColorImage {
    let step = sheet
        .width
        .max(sheet.height)
        .div_ceil(MAX_PREVIEW_SIDE)
        .max(1);
    let (w, h) = (sheet.width.div_ceil(step), sheet.height.div_ceil(step));
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = sheet.pixels[((y * step) * sheet.width + x * step) as usize];
            pixels.push(Color32::from_rgba_unmultiplied(p.r, p.g, p.b, p.a));
        }
    }
    egui::ColorImage {
        size: [w as usize, h as usize],
        pixels,
    }
}

fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxypixel_core::Rgba;

    #[test]
    fn a_large_sheet_previews_inside_the_texture_limit() {
        let sheet = Sheet {
            width: 4100,
            height: 3,
            pixels: vec![Rgba::new(1, 2, 3, 255); 4100 * 3],
        };
        let image = preview_image(&sheet);
        assert!(image.size[0] as u32 <= MAX_PREVIEW_SIDE);
        assert_eq!(image.size, [1367, 1]);
        assert_eq!(image.pixels.len(), 1367);
    }
}
