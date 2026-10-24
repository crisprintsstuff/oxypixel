//! Reference overlays and the floating panel. Pixels stay out of the document.

use eframe::egui::{
    self, Color32, Painter, Pos2, Rect, Sense, TextureHandle, TextureOptions, Vec2,
};
use oxypixel_core::{Error, References, Rgba, Tool, MAX_REFERENCE_SCALE, MIN_REFERENCE_SCALE};

/// Textures for reference images. Rebuilt only when an id appears.
#[derive(Default)]
pub struct RefTextures {
    images: Vec<(u64, TextureHandle)>,
}

impl RefTextures {
    pub fn sync(&mut self, ctx: &egui::Context, refs: &References) {
        let live: Vec<u64> = refs.ids().collect();
        self.images.retain(|(id, _)| live.contains(id));
        for id in live {
            if self.images.iter().any(|(have, _)| *have == id) {
                continue;
            }
            let Some((width, height, bytes)) = refs.texture_rgba(id) else {
                continue;
            };
            let image =
                egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &bytes);
            let texture =
                ctx.load_texture(format!("oxypixel-ref-{id}"), image, TextureOptions::NEAREST);
            self.images.push((id, texture));
        }
    }

    fn get(&self, id: u64) -> Option<&TextureHandle> {
        self.images
            .iter()
            .find(|(have, _)| *have == id)
            .map(|(_, texture)| texture)
    }

    pub fn clear(&mut self) {
        self.images.clear();
    }
}

pub enum ListAction {
    Copy { name: String, pixels: Vec<Rgba> },
    Error(Error),
}

/// Controls for each reference. Returns a layer copy when the user asks for one.
pub fn show_list(
    ui: &mut egui::Ui,
    refs: &mut References,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<ListAction> {
    let mut action = None;
    ui.separator();
    ui.collapsing("References", |ui| {
        ui.label(
            egui::RichText::new("Not in the .oxypix file, and not in export.")
                .small()
                .weak(),
        );
        if refs.is_empty() {
            ui.label("Load one from the File menu, or drop a JPEG.");
            return;
        }
        let count = refs.len();
        let mut removed = false;
        for index in 0..count {
            let selected = refs.active() == Some(index);
            let name = refs.name(index).unwrap_or_else(|| "reference".to_string());
            if ui.selectable_label(selected, name).clicked() {
                refs.set_active(Some(index));
            }
            let mut visible = refs.visible(index);
            let mut locked = refs.locked(index);
            let mut behind = refs.behind(index);
            let mut panel = refs.panel(index);
            let mut sample = refs.samples(index);
            ui.horizontal(|ui| {
                if ui.checkbox(&mut visible, "Show").changed() {
                    let _ = refs.set_visible(index, visible);
                }
                if ui.checkbox(&mut locked, "Lock").changed() {
                    let _ = refs.set_locked(index, locked);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut behind, "Behind")
                    .on_hover_text("Draw under the art. Off draws it on top.")
                    .changed()
                {
                    let _ = refs.set_behind(index, behind);
                }
                if ui
                    .checkbox(&mut panel, "Panel")
                    .on_hover_text("Show this image in a floating window instead of on the canvas.")
                    .changed()
                {
                    let _ = refs.set_panel(index, panel);
                }
            });
            let mut opacity = refs.opacity(index);
            if ui
                .add(egui::Slider::new(&mut opacity, 0..=255).text("Opacity"))
                .changed()
            {
                let _ = refs.set_opacity(index, opacity);
            }
            let mut scale = refs.scale(index);
            if ui
                .add(
                    egui::Slider::new(&mut scale, MIN_REFERENCE_SCALE..=MAX_REFERENCE_SCALE)
                        .logarithmic(true)
                        .max_decimals(3)
                        .text("Scale"),
                )
                .changed()
            {
                let _ = refs.set_scale(index, scale);
            }
            if let Some((mut width, mut height)) = refs.display_size(index) {
                ui.horizontal(|ui| {
                    ui.label("Size");
                    let w = ui
                        .add(egui::DragValue::new(&mut width).speed(1.0).max_decimals(1))
                        .on_hover_text("Width on the canvas, in pixels. The height follows.");
                    ui.label("×");
                    let h = ui
                        .add(egui::DragValue::new(&mut height).speed(1.0).max_decimals(1))
                        .on_hover_text("Height on the canvas, in pixels. The width follows.");
                    ui.label("px");
                    if w.changed() {
                        let _ = refs.set_display_width(index, width);
                    } else if h.changed() {
                        let _ = refs.set_display_height(index, height);
                    }
                });
            }
            if let Some((x, y)) = refs.placement(index) {
                ui.label(format!("{x:.0}, {y:.0}"))
                    .on_hover_text("Document position. Ctrl+drag the image on the canvas to move it.");
            }
            if ui
                .checkbox(&mut sample, "Sample")
                .on_hover_text(
                    "Let the eyedropper copy this image. Export, merge, and the preview still skip it.",
                )
                .changed()
            {
                let _ = refs.set_sample(index, sample);
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!refs.missing(index), egui::Button::new("Fit to canvas"))
                    .on_hover_text("Scale the image to fit the canvas and center it.")
                    .clicked()
                {
                    if let Err(err) = refs.fit_to_canvas(index, canvas_w, canvas_h) {
                        action = Some(ListAction::Error(err));
                    }
                }
                if ui
                    .add_enabled(!refs.missing(index), egui::Button::new("Copy onto layer"))
                    .on_hover_text(
                        "Puts these pixels on a new layer. That layer is part of the document. The reference stays out of the file.",
                    )
                    .clicked()
                {
                    match refs.bake(index, canvas_w, canvas_h) {
                        Ok(pixels) => {
                            let name = refs
                                .name(index)
                                .unwrap_or_else(|| "Reference".to_string())
                                .replace(" (missing)", "");
                            action = Some(ListAction::Copy { name, pixels });
                        }
                        Err(err) => action = Some(ListAction::Error(err)),
                    }
                }
                if ui.button("Remove").clicked() {
                    let _ = refs.remove(index);
                    removed = true;
                }
            });
            if removed {
                break;
            }
            ui.separator();
        }
    });
    action
}

pub fn paint_overlays(
    painter: &Painter,
    refs: &References,
    textures: &RefTextures,
    origin: Pos2,
    zoom: f32,
    behind: bool,
) {
    if zoom <= 0.0 {
        return;
    }
    for overlay in refs.overlays() {
        if overlay.behind != behind {
            continue;
        }
        let Some(texture) = textures.get(overlay.id) else {
            continue;
        };
        let rect = Rect::from_min_size(
            origin + Vec2::new(overlay.x * zoom, overlay.y * zoom),
            Vec2::new(
                overlay.width as f32 * overlay.scale * zoom,
                overlay.height as f32 * overlay.scale * zoom,
            ),
        );
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::from_rgba_unmultiplied(255, 255, 255, overlay.opacity),
        );
    }
}

pub fn paint_active_outline(painter: &Painter, refs: &References, origin: Pos2, zoom: f32) {
    if zoom <= 0.0 {
        return;
    }
    for overlay in refs.overlays() {
        if !overlay.active {
            continue;
        }
        let rect = Rect::from_min_size(
            origin + Vec2::new(overlay.x * zoom, overlay.y * zoom),
            Vec2::new(
                overlay.width as f32 * overlay.scale * zoom,
                overlay.height as f32 * overlay.scale * zoom,
            ),
        );
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0_f32, crate::theme::AMBER),
            egui::StrokeKind::Outside,
        );
    }
}

/// Floating windows for references that are in panel mode.
///
/// A click with the eyedropper, or Alt held, copies a pixel only when Sample is on.
pub fn show_panels(
    ctx: &egui::Context,
    refs: &References,
    textures: &RefTextures,
    tool: Tool,
    alt: bool,
) -> Option<(Rgba, bool)> {
    let mut picked = None;
    for (id, opacity, _sample, width, height, name) in refs.panels() {
        let Some(texture) = textures.get(id) else {
            continue;
        };
        egui::Window::new(name)
            .id(egui::Id::new(("oxypixel-ref", id)))
            .resizable(true)
            .default_size([220.0, 220.0])
            .show(ctx, |ui| {
                let max_w = ui.available_width().max(32.0);
                let sx = max_w / width.max(1) as f32;
                let sy = 220.0 / height.max(1) as f32;
                let scale = sx.min(sy).clamp(0.05, 8.0);
                let size = Vec2::new(width as f32 * scale, height as f32 * scale);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_rgba_unmultiplied(255, 255, 255, opacity),
                );
                let want = tool == Tool::Eyedropper || alt;
                if want && (response.clicked() || response.secondary_clicked()) {
                    if let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) {
                        if rect.width() > 0.0 && rect.height() > 0.0 {
                            let u = (pos.x - rect.min.x) / rect.width();
                            let v = (pos.y - rect.min.y) / rect.height();
                            if let Some(color) = refs.panel_color(id, u, v) {
                                picked = Some((color, response.secondary_clicked()));
                            }
                        }
                    }
                }
            });
    }
    picked
}
