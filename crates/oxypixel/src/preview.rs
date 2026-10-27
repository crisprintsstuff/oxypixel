//! A small preview of the whole composite, beside the canvas.
//!
//! The picture uses an integer scale that fits the panel. Playback walks the
//! frame durations without changing the frame being edited, and it reuses a
//! composite that is already cached.

use eframe::egui::{
    self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};
use oxypixel_core::{advance, Editor, Rgba};

use crate::cache::FrameComposites;
use crate::canvas::{self, ViewState};
use crate::theme::AMBER;

pub struct PreviewPanel {
    pub open: bool,
    playing: bool,
    frame: usize,
    acc: u32,
    last_time: Option<f64>,
    texture: Option<TextureHandle>,
    tex_gen: u64,
    tex_frame: usize,
    tex_w: u32,
    tex_h: u32,
    tex_live: bool,
}

impl PreviewPanel {
    /// Rebuild the picture on the next draw, after the checkerboard changed.
    pub fn invalidate(&mut self) {
        self.texture = None;
    }

    pub fn new(open: bool) -> Self {
        Self {
            open,
            playing: false,
            frame: 0,
            acc: 0,
            last_time: None,
            texture: None,
            tex_gen: 0,
            tex_frame: usize::MAX,
            tex_w: 0,
            tex_h: 0,
            tex_live: false,
        }
    }

    pub fn tick(&mut self, ctx: &egui::Context, editor: &Editor) {
        if !self.open {
            self.playing = false;
            self.last_time = None;
            return;
        }
        let count = editor.document().frame_count();
        if count < 2 {
            self.playing = false;
        }
        let now = ctx.input(|input| input.time);
        if self.playing {
            if self.frame >= count {
                self.frame = 0;
                self.acc = 0;
            }
            if let Some(prev) = self.last_time {
                let dt = ((now - prev) * 1000.0).clamp(0.0, 200.0) as u32;
                let durations: Vec<u32> = (0..count)
                    .map(|index| {
                        editor
                            .document()
                            .frame_duration(index)
                            .unwrap_or(oxypixel_core::DEFAULT_FRAME_MS)
                    })
                    .collect();
                let (next, acc) = advance(self.frame, &durations, self.acc, dt);
                self.frame = next;
                self.acc = acc;
            }
            ctx.request_repaint();
        }
        self.last_time = Some(now);
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        editor: &Editor,
        view: &mut ViewState,
        composites: &mut FrameComposites,
    ) {
        if !self.open {
            return;
        }
        egui::SidePanel::right("preview")
            .resizable(true)
            .default_width(188.0)
            .width_range(140.0..=320.0)
            .show(ctx, |ui| {
                self.contents(ui, editor, view, composites);
            });
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        editor: &Editor,
        view: &mut ViewState,
        composites: &mut FrameComposites,
    ) {
        ui.heading("Preview");
        let doc = editor.document();
        let width = doc.width();
        let height = doc.height();
        let count = doc.frame_count();
        let displayed = if self.playing {
            self.frame.min(count.saturating_sub(1))
        } else {
            doc.active_frame()
        };
        let avail = ui.available_size();
        let image_h = (avail.y - 36.0).max(72.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(avail.x, image_h), Sense::click_and_drag());
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(20, 22, 30));
        let scale =
            canvas::integer_fit_scale(rect.width() - 8.0, rect.height() - 8.0, width, height);
        let drawn = Vec2::new(width as f32 * scale, height as f32 * scale);
        let origin = rect.center() - drawn * 0.5;
        self.ensure_texture(ui, editor, composites, displayed, width, height);
        if let Some(texture) = &self.texture {
            ui.painter().image(
                texture.id(),
                Rect::from_min_size(origin, drawn),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if view.panel.width() > 1.0 && view.zoom > 0.0 {
            let visible = canvas::visible_document_bounds(view, width, height);
            let screen = Rect::from_min_max(
                origin + Vec2::new(visible.min.x * scale, visible.min.y * scale),
                origin + Vec2::new(visible.max.x * scale, visible.max.y * scale),
            );
            ui.painter()
                .rect_stroke(screen, 0.0, Stroke::new(1.0_f32, AMBER), StrokeKind::Inside);
        }
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some((x, y)) = document_point(origin, scale, width, height, pos) {
                    canvas::center_document_point(view, x + 0.5, y + 0.5);
                }
            }
        }
        ui.horizontal(|ui| {
            let can_play = count > 1;
            let label = if self.playing { "Pause" } else { "Play" };
            if ui
                .add_enabled(can_play, egui::Button::new(label))
                .on_hover_text("Play the frames in this panel, using each frame's duration")
                .clicked()
            {
                self.playing = !self.playing;
                if self.playing {
                    self.frame = doc.active_frame();
                    self.acc = 0;
                    self.last_time = None;
                }
            }
            let shown = displayed + 1;
            if self.playing {
                ui.label(format!("Playing {shown}/{count}"));
            } else {
                ui.label(format!("Frame {shown}/{count}"));
            }
        });
    }

    fn ensure_texture(
        &mut self,
        ui: &egui::Ui,
        editor: &Editor,
        composites: &mut FrameComposites,
        frame: usize,
        width: u32,
        height: u32,
    ) {
        let generation = editor.generation();
        let live = editor.gesture_active() && frame == editor.document().active_frame();
        let stale = self.texture.is_none()
            || self.tex_gen != generation
            || self.tex_frame != frame
            || self.tex_w != width
            || self.tex_h != height
            || live
            || self.tex_live;
        self.tex_live = live;
        if !stale {
            return;
        }
        composites.prepare(editor, &[frame]);
        let Some(pixels) = composites.pixels(frame) else {
            return;
        };
        let mut rgba = canvas::checker_image(pixels, width, height);
        if live {
            paint_overlay(&mut rgba, width, height, &editor.preview());
        }
        let image =
            egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
        if let Some(texture) = &mut self.texture {
            if self.tex_w == width && self.tex_h == height {
                texture.set(image, TextureOptions::NEAREST);
            } else {
                self.texture = Some(ui.ctx().load_texture(
                    "oxypixel-preview",
                    image,
                    TextureOptions::NEAREST,
                ));
            }
        } else {
            self.texture = Some(ui.ctx().load_texture(
                "oxypixel-preview",
                image,
                TextureOptions::NEAREST,
            ));
        }
        self.tex_gen = generation;
        self.tex_frame = frame;
        self.tex_w = width;
        self.tex_h = height;
    }
}

fn paint_overlay(rgba: &mut [u8], width: u32, height: u32, marks: &[(i32, i32, Rgba)]) {
    for (x, y, px) in marks {
        if *x < 0 || *y < 0 || *x as u32 >= width || *y as u32 >= height {
            continue;
        }
        let i = (*y as u32 * width + *x as u32) as usize * 4;
        if i + 3 >= rgba.len() {
            continue;
        }
        let dst = Rgba::new(rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]);
        let blended = oxypixel_core::source_over(dst, *px, 255);
        rgba[i..i + 4].copy_from_slice(&blended.to_bytes());
    }
}

/// Document pixel under a preview click. `None` when the pointer misses the picture.
pub(crate) fn document_point(
    origin: Pos2,
    scale: f32,
    width: u32,
    height: u32,
    pos: Pos2,
) -> Option<(f32, f32)> {
    if scale <= 0.0 || width == 0 || height == 0 {
        return None;
    }
    let x = (pos.x - origin.x) / scale;
    let y = (pos.y - origin.y) / scale;
    if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
        return None;
    }
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_maps_into_the_document_and_ignores_the_margin() {
        let origin = Pos2::new(10.0, 20.0);
        let hit = document_point(origin, 2.0, 8, 8, Pos2::new(14.0, 24.0)).expect("pixel");
        assert!((hit.0 - 2.0).abs() < 1e-4);
        assert!((hit.1 - 2.0).abs() < 1e-4);
        assert!(document_point(origin, 2.0, 8, 8, Pos2::new(9.0, 20.0)).is_none());
        assert!(document_point(origin, 2.0, 8, 8, Pos2::new(26.0, 20.0)).is_none());
    }
}
