//! Canvas view: nearest-neighbor pixels, pan, zoom, and pointer tools.

use eframe::egui::{
    self, Color32, Painter, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Vec2,
};
use oxypixel_core::{source_over, Editor, MouseButton, Rgba, Tool};

use crate::theme::{self, AMBER};

pub struct ViewState {
    pub zoom: f32,
    pub pan: Vec2,
    pub grid: bool,
    pub onion: bool,
    pub fit_next: bool,
    pub panel: Rect,
    texture: Option<TextureHandle>,
    cache_valid: bool,
    cached_gen: u64,
    cached_frame: usize,
    cached_onion: bool,
    cached_w: u32,
    cached_h: u32,
    panning: bool,
    sampling: bool,
    pan_last: Pos2,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 8.0,
            pan: Vec2::ZERO,
            grid: true,
            onion: false,
            fit_next: true,
            panel: Rect::ZERO,
            texture: None,
            cache_valid: false,
            cached_gen: 0,
            cached_frame: 0,
            cached_onion: false,
            cached_w: 0,
            cached_h: 0,
            panning: false,
            sampling: false,
            pan_last: Pos2::ZERO,
        }
    }
}

impl ViewState {
    pub fn fit(&mut self, width: u32, height: u32) {
        if self.panel.width() < 8.0 || self.panel.height() < 8.0 || width == 0 || height == 0 {
            return;
        }
        let zx = self.panel.width() / width as f32;
        let zy = self.panel.height() / height as f32;
        self.zoom = (zx.min(zy) * 0.92).clamp(0.1, 64.0);
        let size = Vec2::new(width as f32 * self.zoom, height as f32 * self.zoom);
        self.pan = Vec2::new(
            (self.panel.width() - size.x) * 0.5,
            (self.panel.height() - size.y) * 0.5,
        );
        self.fit_next = false;
    }

    pub fn zoom_at(&mut self, pointer: Pos2, factor: f32) {
        let origin = self.panel.min + self.pan;
        let before = (pointer - origin) / self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.1, 64.0);
        let after = pointer - before * self.zoom;
        self.pan = after - self.panel.min;
    }

    pub fn zoom_center(&mut self, factor: f32) {
        if self.panel.width() < 8.0 {
            self.zoom = (self.zoom * factor).clamp(0.1, 64.0);
            return;
        }
        self.zoom_at(self.panel.center(), factor);
    }
}

pub fn show_canvas(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    view: &mut ViewState,
    allow_draw: bool,
) -> Option<(i32, i32)> {
    let width = editor.document().width();
    let height = editor.document().height();
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    view.panel = rect;
    if view.fit_next {
        view.fit(width, height);
    }
    ensure_texture(ui, editor, view, width, height);

    let origin = rect.min + view.pan;
    let screen = Vec2::new(width as f32 * view.zoom, height as f32 * view.zoom);
    let doc_rect = Rect::from_min_size(origin, screen);
    let painter = ui.painter_at(rect);
    if let Some(texture) = &view.texture {
        painter.image(
            texture.id(),
            doc_rect,
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    draw_grid(&painter, view, origin, rect, width, height);
    if let Some(sel) = editor.selection_preview() {
        let r = Rect::from_min_max(
            origin + Vec2::new(sel.x as f32 * view.zoom, sel.y as f32 * view.zoom),
            origin
                + Vec2::new(
                    (sel.x + sel.width as i32) as f32 * view.zoom,
                    (sel.y + sel.height as i32) as f32 * view.zoom,
                ),
        );
        painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, AMBER), StrokeKind::Inside);
    }

    let hover = response
        .hover_pos()
        .and_then(|pos| pixel_inside(origin, view.zoom, width, height, pos));
    if response.hovered() {
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let icon = if view.panning {
            egui::CursorIcon::Grabbing
        } else if space {
            egui::CursorIcon::Grab
        } else if !allow_draw {
            egui::CursorIcon::NotAllowed
        } else {
            egui::CursorIcon::Crosshair
        };
        ui.ctx().set_cursor_icon(icon);
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(pos) = response.hover_pos() {
                let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                view.zoom_at(pos, factor);
            }
        }
    }
    if let Some((x, y)) = hover {
        if let Some(mark) = hover_rect(editor.tool(), editor.brush(), x, y, origin, view.zoom) {
            painter.rect_stroke(
                mark,
                0.0,
                Stroke::new(1.0_f32, theme::TEAL),
                StrokeKind::Inside,
            );
        }
    }
    if !allow_draw {
        paint_playback_banner(&painter, rect);
    }

    handle_pointer(ui, &response, editor, view, origin, allow_draw);
    hover
}

/// Brush-sized tools preview the stamp. Point tools preview the single pixel under the cursor.
fn hover_rect(tool: Tool, brush: u32, x: i32, y: i32, origin: Pos2, zoom: f32) -> Option<Rect> {
    let (left, top, size) = match tool {
        Tool::Pencil | Tool::Eraser | Tool::Line | Tool::Rectangle | Tool::Ellipse => {
            let size = brush.max(1);
            let half = (size / 2) as i32;
            (x - half, y - half, size)
        }
        Tool::Bucket | Tool::Eyedropper | Tool::Select => (x, y, 1),
    };
    Some(Rect::from_min_size(
        origin + Vec2::new(left as f32 * zoom, top as f32 * zoom),
        Vec2::splat(size as f32 * zoom),
    ))
}

fn paint_playback_banner(painter: &Painter, panel: Rect) {
    let banner = Rect::from_center_size(
        Pos2::new(panel.center().x, panel.min.y + 18.0),
        Vec2::new(240.0, 24.0),
    );
    painter.rect_filled(
        banner,
        4.0,
        Color32::from_rgba_unmultiplied(20, 22, 30, 230),
    );
    painter.text(
        banner.center(),
        egui::Align2::CENTER_CENTER,
        "Playing. Drawing is paused.",
        egui::FontId::proportional(13.0),
        AMBER,
    );
}

fn handle_pointer(
    ui: &egui::Ui,
    response: &egui::Response,
    editor: &mut Editor,
    view: &mut ViewState,
    origin: Pos2,
    allow_draw: bool,
) {
    let space = ui.input(|i| i.key_down(egui::Key::Space));
    let alt = ui.input(|i| i.modifiers.alt);
    let shift = ui.input(|i| i.modifiers.shift);
    let secondary = ui.input(|i| i.pointer.button_down(PointerButton::Secondary));
    let middle = ui.input(|i| i.pointer.button_down(PointerButton::Middle));
    let button = if secondary {
        MouseButton::Secondary
    } else {
        MouseButton::Primary
    };

    if response.drag_started() && (middle || space) {
        view.panning = true;
        view.pan_last = response.interact_pointer_pos().unwrap_or(origin);
        return;
    }
    if view.panning {
        if let Some(pos) = response.interact_pointer_pos() {
            view.pan += pos - view.pan_last;
            view.pan_last = pos;
        }
        if response.drag_stopped() {
            view.panning = false;
        }
        return;
    }
    if !allow_draw {
        return;
    }

    let at = |pos: Pos2| pixel_at(origin, view.zoom, pos);
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (x, y) = at(pos);
            if alt {
                editor.sample(x, y, button);
                view.sampling = true;
            } else {
                editor.pointer_down(x, y, button, shift);
            }
        }
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (x, y) = at(pos);
            if view.sampling || alt {
                editor.sample(x, y, button);
            } else {
                editor.pointer_move(x, y, shift);
            }
        }
    }
    if response.drag_stopped() {
        if let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) {
            let (x, y) = at(pos);
            if !view.sampling {
                editor.pointer_up(x, y, shift);
            }
        }
        view.sampling = false;
    } else if response.clicked() || response.secondary_clicked() {
        if let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) {
            let (x, y) = at(pos);
            let button = if response.secondary_clicked() {
                MouseButton::Secondary
            } else {
                MouseButton::Primary
            };
            if alt {
                editor.sample(x, y, button);
            } else {
                editor.pointer_down(x, y, button, shift);
                editor.pointer_up(x, y, shift);
            }
        }
    }
}

fn ensure_texture(ui: &egui::Ui, editor: &Editor, view: &mut ViewState, width: u32, height: u32) {
    let frame = editor.document().active_frame();
    let live = editor.gesture_active();
    let stale = !view.cache_valid
        || view.cached_gen != editor.generation()
        || view.cached_frame != frame
        || view.cached_onion != view.onion
        || view.cached_w != width
        || view.cached_h != height;
    if !live && !stale {
        return;
    }
    if view.cached_w != width || view.cached_h != height {
        view.texture = None;
    }
    let rgba = display_rgba(editor, view.onion);
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
    if let Some(texture) = &mut view.texture {
        texture.set(image, TextureOptions::NEAREST);
    } else {
        view.texture = Some(ui.ctx().load_texture(
            "oxypixel-canvas",
            image,
            TextureOptions::NEAREST,
        ));
    }
    view.cached_w = width;
    view.cached_h = height;
    if live {
        view.cache_valid = false;
    } else {
        view.cache_valid = true;
        view.cached_gen = editor.generation();
        view.cached_frame = frame;
        view.cached_onion = view.onion;
    }
}

fn display_rgba(editor: &Editor, onion: bool) -> Vec<u8> {
    let doc = editor.document();
    let width = doc.width();
    let height = doc.height();
    let frame = doc.active_frame();
    let current = doc.composite(frame).unwrap_or_default();
    let previous = if onion && doc.frame_count() > 1 {
        let prev = if frame == 0 {
            doc.frame_count() - 1
        } else {
            frame - 1
        };
        doc.composite(prev).ok()
    } else {
        None
    };
    let mut out = Vec::with_capacity(current.len() * 4);
    for (i, px) in current.iter().enumerate() {
        let x = (i as u32) % width;
        let y = (i as u32) / width;
        let mut color = checker(x, y);
        if let Some(prev) = &previous {
            if let Some(ghost) = prev.get(i) {
                color = source_over(color, ghost.fade(80), 255);
            }
        }
        color = source_over(color, *px, 255);
        out.extend_from_slice(&color.to_bytes());
    }
    for (x, y, px) in editor.preview() {
        if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
            continue;
        }
        let i = (y as u32 * width + x as u32) as usize * 4;
        let dst = Rgba::new(out[i], out[i + 1], out[i + 2], out[i + 3]);
        let blended = source_over(dst, px, 255);
        out[i..i + 4].copy_from_slice(&blended.to_bytes());
    }
    out
}

fn checker(x: u32, y: u32) -> Rgba {
    let color = if ((x / 4) + (y / 4)) % 2 == 0 {
        theme::CHECKER_A
    } else {
        theme::CHECKER_B
    };
    Rgba::new(color.r(), color.g(), color.b(), 255)
}

fn draw_grid(
    painter: &Painter,
    view: &ViewState,
    origin: Pos2,
    panel: Rect,
    width: u32,
    height: u32,
) {
    if !view.grid || view.zoom < 6.0 {
        return;
    }
    let x0 = ((panel.min.x - origin.x) / view.zoom)
        .floor()
        .clamp(0.0, width as f32) as i32;
    let x1 = ((panel.max.x - origin.x) / view.zoom)
        .ceil()
        .clamp(0.0, width as f32) as i32;
    let y0 = ((panel.min.y - origin.y) / view.zoom)
        .floor()
        .clamp(0.0, height as f32) as i32;
    let y1 = ((panel.max.y - origin.y) / view.zoom)
        .ceil()
        .clamp(0.0, height as f32) as i32;
    if x1 - x0 > 400 || y1 - y0 > 400 {
        return;
    }
    let stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 28));
    let top = origin.y.max(panel.min.y);
    let bottom = (origin.y + height as f32 * view.zoom).min(panel.max.y);
    let left = origin.x.max(panel.min.x);
    let right = (origin.x + width as f32 * view.zoom).min(panel.max.x);
    for x in x0..=x1 {
        let sx = origin.x + x as f32 * view.zoom;
        painter.line_segment([Pos2::new(sx, top), Pos2::new(sx, bottom)], stroke);
    }
    for y in y0..=y1 {
        let sy = origin.y + y as f32 * view.zoom;
        painter.line_segment([Pos2::new(left, sy), Pos2::new(right, sy)], stroke);
    }
}

fn pixel_at(origin: Pos2, zoom: f32, pos: Pos2) -> (i32, i32) {
    let rel = pos - origin;
    ((rel.x / zoom).floor() as i32, (rel.y / zoom).floor() as i32)
}

fn pixel_inside(origin: Pos2, zoom: f32, width: u32, height: u32, pos: Pos2) -> Option<(i32, i32)> {
    let (x, y) = pixel_at(origin, zoom, pos);
    if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
        Some((x, y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_tools_preview_the_stamp_and_point_tools_preview_one_pixel() {
        let origin = Pos2::ZERO;
        let brush = hover_rect(Tool::Pencil, 3, 4, 5, origin, 10.0).unwrap();
        assert_eq!(brush.min, Pos2::new(30.0, 40.0));
        assert_eq!(brush.size(), Vec2::splat(30.0));
        let pixel = hover_rect(Tool::Eyedropper, 8, 2, 2, origin, 10.0).unwrap();
        assert_eq!(pixel.min, Pos2::new(20.0, 20.0));
        assert_eq!(pixel.size(), Vec2::splat(10.0));
        assert!(hover_rect(Tool::Bucket, 4, 0, 0, origin, 2.0).is_some());
        assert!(hover_rect(Tool::Select, 4, 0, 0, origin, 2.0).is_some());
    }
}
