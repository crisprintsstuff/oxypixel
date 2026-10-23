//! Frame strip thumbnails. Names are not stored: a version 1 file has no frame-name field.

use eframe::egui::{
    self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};
use oxypixel_core::Editor;

use crate::cache::FrameComposites;
use crate::canvas::{self, fitted_rgba};
use crate::theme::AMBER;

/// Slider bounds. The same 20–10000 ms range the file format stores.
pub(crate) fn duration_limits() -> (u32, u32) {
    (oxypixel_core::MIN_FRAME_MS, oxypixel_core::MAX_FRAME_MS)
}

struct ThumbSlot {
    stamp: u64,
    width: u32,
    height: u32,
    texture: TextureHandle,
}

#[derive(Default)]
pub struct ThumbCache {
    doc_w: u32,
    doc_h: u32,
    count: usize,
    slots: Vec<Option<ThumbSlot>>,
}

const THUMB_BOX: u32 = 36;
const CELL: Vec2 = Vec2::new(44.0, 52.0);

impl ThumbCache {
    fn image(
        &mut self,
        ui: &egui::Ui,
        editor: &Editor,
        composites: &mut FrameComposites,
        index: usize,
    ) -> Option<(TextureHandle, u32, u32)> {
        let doc = editor.document();
        let width = doc.width();
        let height = doc.height();
        let count = doc.frame_count();
        if self.doc_w != width || self.doc_h != height || self.count != count {
            self.doc_w = width;
            self.doc_h = height;
            self.count = count;
            self.slots = (0..count).map(|_| None).collect();
        }
        composites.prepare(editor, &[index]);
        let stamp = composites.stamp(index)?;
        if let Some(Some(slot)) = self.slots.get(index) {
            if slot.stamp == stamp {
                return Some((slot.texture.clone(), slot.width, slot.height));
            }
        }
        let pixels = composites.pixels(index)?;
        let (tw, th, rgba) = fitted_rgba(pixels, width, height, THUMB_BOX, THUMB_BOX);
        let image = egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &rgba);
        let texture = if let Some(slot) = self.slots.get_mut(index).and_then(Option::as_mut) {
            slot.texture.set(image, TextureOptions::NEAREST);
            slot.stamp = stamp;
            slot.width = tw;
            slot.height = th;
            slot.texture.clone()
        } else {
            let texture = ui.ctx().load_texture(
                format!("oxypixel-frame-{index}"),
                image,
                TextureOptions::NEAREST,
            );
            if let Some(place) = self.slots.get_mut(index) {
                *place = Some(ThumbSlot {
                    stamp,
                    width: tw,
                    height: th,
                    texture: texture.clone(),
                });
            }
            texture
        };
        Some((texture, tw, th))
    }
}

/// Draw the strip. Returns the frame index that was clicked.
pub fn show(
    ui: &mut egui::Ui,
    editor: &Editor,
    composites: &mut FrameComposites,
    thumbs: &mut ThumbCache,
) -> Option<usize> {
    let count = editor.document().frame_count();
    let active = editor.document().active_frame();
    let mut clicked = None;
    for index in 0..count {
        let (rect, response) = ui.allocate_exact_size(CELL, Sense::click());
        let selected = index == active;
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, Color32::from_rgb(20, 22, 30));
        let image_rect = Rect::from_min_size(rect.min + Vec2::new(4.0, 2.0), Vec2::new(36.0, 36.0));
        if let Some((texture, tw, th)) = thumbs.image(ui, editor, composites, index) {
            let scale = canvas::integer_fit_scale(image_rect.width(), image_rect.height(), tw, th);
            let drawn = Vec2::new(tw as f32 * scale, th as f32 * scale);
            let origin = image_rect.center() - drawn * 0.5;
            painter.image(
                texture.id(),
                Rect::from_min_size(origin, drawn),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let label_color = if selected {
            AMBER
        } else {
            Color32::from_rgb(210, 214, 220)
        };
        painter.text(
            Pos2::new(rect.center().x, rect.max.y - 8.0),
            egui::Align2::CENTER_CENTER,
            format!("{}", index + 1),
            egui::FontId::proportional(11.0),
            label_color,
        );
        let stroke = if selected {
            AMBER
        } else {
            Color32::from_rgb(70, 74, 88)
        };
        painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
        let ms = editor
            .document()
            .frame_duration(index)
            .unwrap_or(oxypixel_core::DEFAULT_FRAME_MS);
        response
            .clone()
            .on_hover_text(format!("Frame {} · {ms} ms", index + 1));
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_slider_reaches_the_file_limit() {
        let (min_ms, max_ms) = duration_limits();
        assert_eq!(min_ms, 20);
        assert_eq!(max_ms, 10_000);
        assert!(max_ms > 1000);
    }
}
