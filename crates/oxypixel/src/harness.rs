//! A window test harness: the whole app on a headless egui context, driven
//! with the same input events a window gets.
//!
//! Widgets are found by the text they draw: each frame's shapes are kept,
//! and [`Harness::click_text`] clicks the middle of the label that reads
//! exactly the given text. The data folder is a fresh temporary one for
//! each harness, so tests never read or write the user's settings, recent
//! files, or recovery copies. Nothing opens a window or needs a GPU.

use std::path::PathBuf;

use eframe::egui::{
    self, epaint::ClippedShape, Event, Key, Modifiers, PointerButton, Pos2, Rect, Shape, Vec2,
};

use crate::app::OxyApp;
use crate::recent::TEST_DATA_DIR;

/// The window size tests run at, in points.
const SCREEN: Vec2 = Vec2::new(1280.0, 800.0);

pub struct Harness {
    pub ctx: egui::Context,
    pub app: OxyApp,
    events: Vec<Event>,
    shapes: Vec<ClippedShape>,
    time: f64,
    dir: PathBuf,
}

impl Harness {
    /// The app as it starts, with the built-in example open.
    pub fn new() -> Self {
        Self::with_file(None)
    }

    /// The app started with `open` as its file argument.
    pub fn with_file(open: Option<PathBuf>) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "oxypixel-harness-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("harness data folder");
        TEST_DATA_DIR.with(|slot| *slot.borrow_mut() = Some(dir.clone()));
        let ctx = egui::Context::default();
        let app = OxyApp::with_context(&ctx, open);
        let mut harness = Self {
            ctx,
            app,
            events: Vec::new(),
            shapes: Vec::new(),
            time: 0.0,
            dir,
        };
        // The canvas fits itself to the panel over the first frames.
        harness.run(3);
        harness
    }

    /// The temporary data folder this harness uses.
    pub fn data_dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Run one frame with the queued input.
    pub fn step(&mut self) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN)),
            time: Some(self.time),
            events: std::mem::take(&mut self.events),
            ..Default::default()
        };
        let app = &mut self.app;
        let output = self.ctx.run(input, |ctx| app.frame(ctx));
        self.shapes = output.shapes;
    }

    pub fn run(&mut self, frames: usize) {
        for _ in 0..frames {
            self.step();
        }
    }

    /// Where the label reading exactly `text` was drawn last frame. The
    /// topmost one wins, so a menu or dialog beats the panel under it.
    pub fn find(&self, text: &str) -> Option<Rect> {
        let mut found = None;
        for clipped in &self.shapes {
            visit(&clipped.shape, &mut |shape| {
                if let Shape::Text(t) = shape {
                    if t.galley.text().trim() == text {
                        found = Some(t.galley.rect.translate(t.pos.to_vec2()));
                    }
                }
            });
        }
        found
    }

    /// Every piece of text drawn last frame, for test failure messages.
    pub fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for clipped in &self.shapes {
            visit(&clipped.shape, &mut |shape| {
                if let Shape::Text(t) = shape {
                    out.push(t.galley.text().to_string());
                }
            });
        }
        out
    }

    /// Click the label reading `text`. Panics with what was on screen if
    /// there is none.
    pub fn click_text(&mut self, text: &str) {
        // A window that just opened sizes and centers itself over its first
        // frames; wait until the label stays put.
        let mut rect = self.find(text);
        for _ in 0..10 {
            self.step();
            let now = self.find(text);
            if now.is_some() && now == rect {
                break;
            }
            rect = now;
        }
        let Some(rect) = rect else {
            panic!("no {text:?} on screen; saw {:?}", self.texts());
        };
        self.click(rect.center());
    }

    pub fn click(&mut self, pos: Pos2) {
        self.press(pos, PointerButton::Primary);
        self.release(pos, PointerButton::Primary);
    }

    pub fn press(&mut self, pos: Pos2, button: PointerButton) {
        self.events.push(Event::PointerMoved(pos));
        self.step();
        self.events.push(Event::PointerButton {
            pos,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        self.step();
    }

    pub fn move_to(&mut self, pos: Pos2) {
        self.events.push(Event::PointerMoved(pos));
        self.step();
    }

    pub fn release(&mut self, pos: Pos2, button: PointerButton) {
        self.events.push(Event::PointerButton {
            pos,
            button,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.step();
        self.step();
    }

    /// Press the left button at `from`, move through `path`, release at the
    /// end. The pointer moves in steps of at most 2 points, as a mouse does,
    /// so a drag starts where it would by hand.
    pub fn drag(&mut self, from: Pos2, path: &[Pos2]) {
        self.press(from, PointerButton::Primary);
        let mut last = from;
        for &pos in path {
            let steps = ((pos - last).length() / 2.0).ceil().max(1.0) as usize;
            for i in 1..=steps {
                self.move_to(last + (pos - last) * (i as f32 / steps as f32));
            }
            last = pos;
        }
        self.release(last, PointerButton::Primary);
    }

    pub fn key(&mut self, key: Key, modifiers: Modifiers) {
        for pressed in [true, false] {
            self.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
        self.step();
        self.step();
    }

    /// The middle of canvas pixel `(x, y)` on screen.
    pub fn canvas_point(&self, x: u32, y: u32) -> Pos2 {
        let view = &self.app.view;
        view.panel.min + view.pan + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * view.zoom
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        TEST_DATA_DIR.with(|slot| *slot.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn visit(shape: &Shape, f: &mut impl FnMut(&Shape)) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| visit(s, f)),
        other => f(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxypixel_core::{Rgba, Tool};

    #[test]
    fn the_window_starts_with_the_example_and_its_panels() {
        let h = Harness::new();
        for label in ["Tools", "Layers", "Palette", "Add frame", "File", "Edit"] {
            assert!(
                h.find(label).is_some(),
                "{label} missing; saw {:?}",
                h.texts()
            );
        }
        assert!(h.app.view.zoom >= 1.0, "the canvas fitted itself");
    }

    #[test]
    fn drawing_on_the_canvas_and_undo() {
        let mut h = Harness::new();
        let layer = h.app.editor.document().active_layer();
        let ink = h.app.editor.primary();
        let at = |h: &Harness| h.app.editor.document().cel(0, layer).unwrap()[3 * 32 + 2];
        assert_ne!(at(&h), ink);
        let (from, to) = (h.canvas_point(2, 3), h.canvas_point(6, 3));
        h.drag(from, &[to]);
        assert_eq!(at(&h), ink, "the pencil drew at (2, 3)");
        assert_eq!(
            h.app.editor.document().cel(0, layer).unwrap()[3 * 32 + 6],
            ink
        );
        h.key(Key::Z, Modifiers::COMMAND);
        assert_ne!(at(&h), ink, "Ctrl+Z took the stroke back");
    }

    #[test]
    fn a_stroke_at_low_zoom_starts_where_the_button_went_down() {
        let mut h = Harness::new();
        h.app.view.zoom = 2.0;
        h.app.view.fit_next = false;
        h.step();
        let layer = h.app.editor.document().active_layer();
        let ink = h.app.editor.primary();
        let (from, to) = (h.canvas_point(1, 20), h.canvas_point(12, 20));
        h.drag(from, &[to]);
        let cel = h.app.editor.document().cel(0, layer).unwrap();
        let drawn: Vec<usize> = (0..32).filter(|&x| cel[20 * 32 + x] == ink).collect();
        assert_eq!(
            drawn,
            (1..=12).collect::<Vec<_>>(),
            "every pixel from the press to the release"
        );
    }

    #[test]
    fn menus_and_dialogs_drive_the_editor() {
        let mut h = Harness::new();
        // Edit, then Replace color…: the primary becomes the second color.
        let from = h.app.editor.primary();
        let count = |h: &Harness, c: Rgba| {
            let doc = h.app.editor.document();
            (0..doc.layer_count())
                .map(|l| doc.cel(0, l).unwrap().iter().filter(|p| **p == c).count())
                .sum::<usize>()
        };
        let before = count(&h, from);
        assert!(before > 0, "the example uses the primary color");
        h.click_text("Edit");
        h.click_text("Replace color…");
        h.click_text("Every layer");
        h.click_text("Replace");
        assert_eq!(count(&h, from), 0, "{:?}", h.texts());
        h.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(count(&h, from), before, "one undo step");

        // The layer panel's Tilemap… adds a tilemap layer with 8×8 tiles.
        h.click_text("Tilemap");
        h.click_text("8×8");
        h.click_text("Add");
        let doc = h.app.editor.document();
        let set = doc
            .layer_tileset(doc.active_layer())
            .expect("a tilemap layer");
        assert_eq!(doc.tilesets()[set].tile_width(), 8);
        assert!(h.find("Tile mode").is_some(), "the Tiles panel shows");
    }

    #[test]
    fn tiles_are_drawn_and_placed_on_the_canvas() {
        let mut h = Harness::new();
        h.click_text("Tilemap");
        h.click_text("8×8");
        h.click_text("Add");
        let layer = h.app.editor.document().active_layer();
        // A pixel on an empty cell makes tile 1 there.
        let p = h.canvas_point(1, 1);
        h.click(p);
        assert_eq!(h.app.editor.document().tile_grid(0, layer).unwrap()[0], 1);
        // Tile mode places it in the cell to the right.
        h.click_text("Tile mode");
        let p = h.canvas_point(12, 2);
        h.click(p);
        let grid = h.app.editor.document().tile_grid(0, layer).unwrap();
        assert_eq!(&grid[..2], &[1, 1]);
        let ink = h.app.editor.primary();
        assert_eq!(h.app.editor.document().cel(0, layer).unwrap()[32 + 9], ink);
    }

    #[test]
    fn the_live_bridge_starts_from_settings_and_answers_in_the_frame_loop() {
        let mut h = Harness::new();
        h.key(Key::Comma, Modifiers::COMMAND);
        h.click_text("Let oxypixel mcp edit the open document");
        let info = h.data_dir().join("bridge.txt");
        assert!(info.is_file(), "the bridge wrote its file");
        let path = info.clone();
        let reader = std::thread::spawn(move || crate::bridge::fetch(Some(&path)));
        // The window answers as it draws frames.
        for _ in 0..500 {
            if reader.is_finished() {
                break;
            }
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let (_, bytes) = reader
            .join()
            .unwrap()
            .expect("the window lent its document");
        let doc = oxypixel_core::load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(doc.width(), h.app.editor.document().width());
        h.click_text("Let oxypixel mcp edit the open document");
        assert!(!info.exists(), "turning it off removes the file");
    }

    #[test]
    fn shortcuts_and_settings() {
        let mut h = Harness::new();
        h.key(Key::E, Modifiers::NONE);
        assert_eq!(h.app.editor.tool(), Tool::Eraser);
        h.key(Key::B, Modifiers::NONE);
        assert_eq!(h.app.editor.tool(), Tool::Pencil);
        // Ctrl+, opens Settings; turning the grid off is kept in the data folder.
        h.key(Key::Comma, Modifiers::COMMAND);
        assert!(h.find("Settings").is_some(), "{:?}", h.texts());
        h.click_text("Show the pixel grid");
        assert!(!h.app.prefs.grid);
        assert!(!h.app.view.grid);
        let saved = std::fs::read_to_string(h.data_dir().join("view.txt")).unwrap();
        assert!(saved.contains("grid=0"), "{saved}");
        h.click_text("Close");
        assert!(h.find("Show the pixel grid").is_none());
    }
}
