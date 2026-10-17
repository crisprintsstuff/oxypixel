//! Editing session: tools, selection, colors, and undo on top of a [`Document`].

use crate::color::Rgba;
use crate::document::Document;
use crate::draw::{
    brush_offsets, clamp_to_margin, constrain_line, constrain_square, ellipse_points, flood_fill,
    line_points, rect_points, PixelRect,
};
use crate::error::{Error, Result};
use crate::history::History;
use crate::limits::{clamp_name, MAX_BRUSH};

/// Drawing tool. Shape tools commit on pointer release. Pencil and eraser paint immediately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Eraser,
    Bucket,
    Eyedropper,
    Line,
    Rectangle,
    Ellipse,
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Primary,
    Secondary,
}

#[derive(Clone, Debug)]
enum Gesture {
    None,
    Pencil {
        button: MouseButton,
        last: (i32, i32),
    },
    Shape {
        tool: Tool,
        button: MouseButton,
        origin: (i32, i32),
        current: (i32, i32),
        shift: bool,
    },
    Select {
        origin: (i32, i32),
        current: (i32, i32),
    },
}

/// Editor state that is not part of the saved document: tool, colors, selection, history.
#[derive(Debug)]
pub struct Editor {
    doc: Document,
    history: History,
    tool: Tool,
    primary: Rgba,
    secondary: Rgba,
    brush: u32,
    shape_filled: bool,
    selection: Option<PixelRect>,
    gesture: Gesture,
    gesture_saved: bool,
    coalescing: bool,
    coalesce_saved: bool,
    dirty: bool,
    generation: u64,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(64, 64).expect("64×64 is within the canvas limits")
    }
}

impl Editor {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Ok(Self::from_document(Document::new(width, height)?))
    }

    pub fn demo() -> Result<Self> {
        Ok(Self::from_document(Document::demo()?))
    }

    pub fn from_document(doc: Document) -> Self {
        let primary = doc
            .palette()
            .get(5)
            .copied()
            .unwrap_or(Rgba::new(232, 168, 56, 255));
        let secondary = doc
            .palette()
            .first()
            .copied()
            .unwrap_or(Rgba::new(20, 22, 30, 255));
        Self {
            doc,
            history: History::default(),
            tool: Tool::Pencil,
            primary,
            secondary,
            brush: 1,
            shape_filled: false,
            selection: None,
            gesture: Gesture::None,
            gesture_saved: false,
            coalescing: false,
            coalesce_saved: false,
            dirty: false,
            generation: 1,
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.cancel_gesture();
        self.tool = tool;
    }

    pub fn brush(&self) -> u32 {
        self.brush
    }

    pub fn set_brush(&mut self, size: u32) {
        self.brush = size.clamp(1, MAX_BRUSH);
    }

    pub fn nudge_brush(&mut self, delta: i32) {
        let next = self.brush as i32 + delta;
        self.set_brush(next.max(1) as u32);
    }

    pub fn shape_filled(&self) -> bool {
        self.shape_filled
    }

    pub fn set_shape_filled(&mut self, filled: bool) {
        self.shape_filled = filled;
    }

    pub fn primary(&self) -> Rgba {
        self.primary
    }

    pub fn secondary(&self) -> Rgba {
        self.secondary
    }

    pub fn set_primary(&mut self, color: Rgba) {
        self.primary = color;
    }

    pub fn set_secondary(&mut self, color: Rgba) {
        self.secondary = color;
    }

    pub fn swap_colors(&mut self) {
        std::mem::swap(&mut self.primary, &mut self.secondary);
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> bool {
        self.cancel_gesture();
        let Some(previous) = self.history.undo(self.doc.clone()) else {
            return false;
        };
        self.doc = previous;
        self.dirty = true;
        self.generation += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        self.cancel_gesture();
        let Some(next) = self.history.redo(self.doc.clone()) else {
            return false;
        };
        self.doc = next;
        self.dirty = true;
        self.generation += 1;
        true
    }

    /// Group the next document edits into one undo step until [`Self::end_coalesce`].
    pub fn begin_coalesce(&mut self) {
        self.coalescing = true;
    }

    pub fn end_coalesce(&mut self) {
        self.coalescing = false;
        self.coalesce_saved = false;
    }

    pub fn gesture_active(&self) -> bool {
        !matches!(self.gesture, Gesture::None)
    }

    pub fn selection(&self) -> Option<PixelRect> {
        self.selection
    }

    /// The rectangle to draw. While a marquee is being dragged this is the drag, not the old selection.
    pub fn selection_preview(&self) -> Option<PixelRect> {
        match self.gesture {
            Gesture::Select { origin, current } => Some(PixelRect::from_corners(
                origin.0, origin.1, current.0, current.1,
            )),
            _ => self.selection,
        }
    }

    pub fn select_all(&mut self) {
        self.cancel_gesture();
        self.selection = Some(PixelRect {
            x: 0,
            y: 0,
            width: self.doc.width(),
            height: self.doc.height(),
        });
    }

    pub fn deselect(&mut self) {
        if matches!(self.gesture, Gesture::Select { .. }) {
            self.gesture = Gesture::None;
        }
        self.selection = None;
    }

    pub fn cancel_gesture(&mut self) {
        self.gesture = Gesture::None;
        self.gesture_saved = false;
    }

    /// Pixels a shape tool would paint. Empty for tools that paint immediately.
    pub fn preview(&self) -> Vec<(i32, i32, Rgba)> {
        let Gesture::Shape {
            tool,
            button,
            origin,
            current,
            shift,
        } = self.gesture
        else {
            return Vec::new();
        };
        let (x1, y1) = self.constrained(tool, origin, current, shift);
        let color = self.button_color(button);
        self.stamp(&self.shape_geometry(tool, origin, (x1, y1)), Some(color))
    }

    pub fn sample(&mut self, x: i32, y: i32, button: MouseButton) -> bool {
        let Some(color) = self.color_at(x, y) else {
            return false;
        };
        match button {
            MouseButton::Primary => self.primary = color,
            MouseButton::Secondary => self.secondary = color,
        }
        true
    }

    pub fn pointer_down(&mut self, x: i32, y: i32, button: MouseButton, shift: bool) {
        let (x, y) = self.clamp_pointer(x, y);
        match self.tool {
            Tool::Pencil | Tool::Eraser => {
                self.gesture = Gesture::Pencil {
                    button,
                    last: (x, y),
                };
                let color = self.stroke_color(button);
                self.paint(&[(x, y)], color, true);
            }
            Tool::Bucket => {
                let color = self.button_color(button);
                self.bucket(x, y, color);
            }
            Tool::Eyedropper => {
                self.sample(x, y, button);
            }
            Tool::Line | Tool::Rectangle | Tool::Ellipse => {
                self.gesture = Gesture::Shape {
                    tool: self.tool,
                    button,
                    origin: (x, y),
                    current: (x, y),
                    shift,
                };
            }
            Tool::Select => {
                self.gesture = Gesture::Select {
                    origin: (x, y),
                    current: (x, y),
                };
            }
        }
    }

    pub fn pointer_move(&mut self, x: i32, y: i32, shift: bool) {
        let (x, y) = self.clamp_pointer(x, y);
        match self.gesture {
            Gesture::Pencil { button, last } => {
                let points = line_points(last.0, last.1, x, y);
                let color = self.stroke_color(button);
                self.paint(&points, color, true);
                self.gesture = Gesture::Pencil {
                    button,
                    last: (x, y),
                };
            }
            Gesture::Shape {
                tool,
                button,
                origin,
                ..
            } => {
                self.gesture = Gesture::Shape {
                    tool,
                    button,
                    origin,
                    current: (x, y),
                    shift,
                };
            }
            Gesture::Select { origin, .. } => {
                self.gesture = Gesture::Select {
                    origin,
                    current: (x, y),
                };
            }
            Gesture::None => {}
        }
    }

    pub fn pointer_up(&mut self, x: i32, y: i32, shift: bool) {
        let (x, y) = self.clamp_pointer(x, y);
        match self.gesture {
            Gesture::Pencil { .. } => {
                self.gesture = Gesture::None;
                self.gesture_saved = false;
            }
            Gesture::Shape {
                tool,
                button,
                origin,
                ..
            } => {
                let end = self.constrained(tool, origin, (x, y), shift);
                let points = self.shape_geometry(tool, origin, end);
                let color = Some(self.button_color(button));
                self.gesture = Gesture::None;
                self.paint(&points, color, false);
                self.gesture_saved = false;
            }
            Gesture::Select { origin, .. } => {
                self.selection = Some(PixelRect::from_corners(origin.0, origin.1, x, y));
                self.gesture = Gesture::None;
            }
            Gesture::None => {}
        }
    }

    pub fn clear_selection_pixels(&mut self) -> Result<()> {
        let Some(sel) = self.selection else {
            return Err(Error::NoSelection);
        };
        self.cancel_gesture();
        let mut points = Vec::new();
        let right = sel.x + sel.width as i32;
        let bottom = sel.y + sel.height as i32;
        for y in sel.y..bottom {
            for x in sel.x..right {
                points.push((x, y));
            }
        }
        self.paint(&points, None, false);
        Ok(())
    }

    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.history.clear();
        self.selection = None;
        self.cancel_gesture();
        self.end_coalesce();
        self.dirty = false;
        self.generation += 1;
    }

    pub fn add_layer(&mut self) -> Result<()> {
        let n = self.doc.layer_count() + 1;
        self.structural(|doc| doc.add_layer(&format!("Layer {n}")))
    }

    pub fn duplicate_layer(&mut self) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.duplicate_layer(index))
    }

    pub fn delete_layer(&mut self) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.delete_layer(index))
    }

    pub fn move_layer(&mut self, delta: i32) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.move_layer(index, delta))
    }

    pub fn merge_down(&mut self) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.merge_down(index))
    }

    pub fn rename_layer(&mut self, name: &str) {
        let index = self.doc.active_layer();
        let name = clamp_name(name);
        if self.doc.layer(index).map(|l| l.name.as_str()) == Some(name.as_str()) {
            return;
        }
        self.mutate(false, |doc| {
            if let Some(layer) = doc.layers_mut().get_mut(index) {
                layer.name = name;
            }
        });
    }

    pub fn set_layer_visible(&mut self, visible: bool) {
        let index = self.doc.active_layer();
        if self.doc.layer(index).map(|l| l.visible) == Some(visible) {
            return;
        }
        self.mutate(false, |doc| {
            if let Some(layer) = doc.layers_mut().get_mut(index) {
                layer.visible = visible;
            }
        });
    }

    pub fn set_layer_opacity(&mut self, opacity: u8) {
        let index = self.doc.active_layer();
        if self.doc.layer(index).map(|l| l.opacity) == Some(opacity) {
            return;
        }
        self.mutate(false, |doc| {
            if let Some(layer) = doc.layers_mut().get_mut(index) {
                layer.opacity = opacity;
            }
        });
    }

    pub fn set_active_layer(&mut self, index: usize) -> Result<()> {
        self.cancel_gesture();
        self.doc.set_active_layer(index)?;
        self.generation += 1;
        Ok(())
    }

    pub fn add_frame(&mut self) -> Result<()> {
        self.structural(|doc| doc.add_frame().map(|_| ()))
    }

    pub fn duplicate_frame(&mut self) -> Result<()> {
        let index = self.doc.active_frame();
        self.structural(|doc| doc.duplicate_frame(index).map(|_| ()))
    }

    pub fn delete_frame(&mut self) -> Result<()> {
        let index = self.doc.active_frame();
        self.structural(|doc| doc.delete_frame(index))
    }

    pub fn set_active_frame(&mut self, index: usize) -> Result<()> {
        self.cancel_gesture();
        self.doc.set_active_frame(index)?;
        self.generation += 1;
        Ok(())
    }

    pub fn set_frame_duration(&mut self, ms: u32) {
        let index = self.doc.active_frame();
        if self.doc.frame_duration(index) == Some(crate::limits::clamp_duration(ms))
            && self.doc.frame_duration(index) == Some(ms)
        {
            return;
        }
        let current = self.doc.frame_duration(index);
        let clamped = crate::limits::clamp_duration(ms);
        if current == Some(clamped) {
            return;
        }
        self.mutate(false, |doc| {
            let _ = doc.set_frame_duration(index, ms);
        });
    }

    pub fn add_palette_color(&mut self, color: Rgba) -> Result<()> {
        self.structural(|doc| doc.add_palette_color(color))
    }

    pub fn remove_palette_color(&mut self, index: usize) -> Result<()> {
        self.structural(|doc| doc.remove_palette_color(index))
    }

    /// Add a layer on the active frame and copy `pixels` into it.
    /// `pixels` must already match the document size.
    pub fn insert_layer_pixels(&mut self, name: &str, pixels: &[Rgba]) -> Result<()> {
        let expected = (self.doc.width() * self.doc.height()) as usize;
        if pixels.len() != expected {
            return Err(Error::Limit {
                what: "imported pixels",
                value: pixels.len() as u64,
                max: expected as u64,
            });
        }
        let frame = self.doc.active_frame();
        let owned = pixels.to_vec();
        let name = name.to_string();
        self.structural(|doc| {
            doc.add_layer(&name)?;
            let layer = doc.active_layer();
            doc.cel_mut(frame, layer)?.copy_from_slice(&owned);
            Ok(())
        })
    }

    fn structural(&mut self, f: impl FnOnce(&mut Document) -> Result<()>) -> Result<()> {
        self.cancel_gesture();
        let snapshot = self.doc.clone();
        match f(&mut self.doc) {
            Ok(()) => {
                self.history.push(snapshot);
                self.dirty = true;
                self.generation += 1;
                Ok(())
            }
            Err(err) => {
                self.doc = snapshot;
                Err(err)
            }
        }
    }

    fn mutate(&mut self, coalesce: bool, f: impl FnOnce(&mut Document)) {
        self.cancel_gesture();
        let grouped = coalesce || self.coalescing;
        if !(grouped && self.coalesce_saved) {
            self.history.push(self.doc.clone());
            if grouped {
                self.coalesce_saved = true;
            }
        }
        f(&mut self.doc);
        self.dirty = true;
        self.generation += 1;
    }

    fn clamp_pointer(&self, x: i32, y: i32) -> (i32, i32) {
        let margin = self.brush as i32 + 2;
        (
            clamp_to_margin(x, self.doc.width() as i32, margin),
            clamp_to_margin(y, self.doc.height() as i32, margin),
        )
    }

    fn button_color(&self, button: MouseButton) -> Rgba {
        match button {
            MouseButton::Primary => self.primary,
            MouseButton::Secondary => self.secondary,
        }
    }

    fn stroke_color(&self, button: MouseButton) -> Option<Rgba> {
        if self.tool == Tool::Eraser {
            None
        } else {
            Some(self.button_color(button))
        }
    }

    fn color_at(&self, x: i32, y: i32) -> Option<Rgba> {
        if x < 0 || y < 0 || x as u32 >= self.doc.width() || y as u32 >= self.doc.height() {
            return None;
        }
        let pixels = self.doc.composite(self.doc.active_frame()).ok()?;
        let i = (y as u32 * self.doc.width() + x as u32) as usize;
        pixels.get(i).copied()
    }

    fn constrained(
        &self,
        tool: Tool,
        origin: (i32, i32),
        current: (i32, i32),
        shift: bool,
    ) -> (i32, i32) {
        if !shift {
            return current;
        }
        match tool {
            Tool::Line => constrain_line(origin.0, origin.1, current.0, current.1),
            Tool::Rectangle | Tool::Ellipse => {
                constrain_square(origin.0, origin.1, current.0, current.1)
            }
            _ => current,
        }
    }

    fn shape_geometry(&self, tool: Tool, origin: (i32, i32), end: (i32, i32)) -> Vec<(i32, i32)> {
        match tool {
            Tool::Line => line_points(origin.0, origin.1, end.0, end.1),
            Tool::Rectangle => rect_points(origin.0, origin.1, end.0, end.1, self.shape_filled),
            Tool::Ellipse => ellipse_points(origin.0, origin.1, end.0, end.1, self.shape_filled),
            _ => Vec::new(),
        }
    }

    /// `color == None` erases (writes clear). Brush and selection are applied here.
    fn paint(&mut self, points: &[(i32, i32)], color: Option<Rgba>, coalesce: bool) {
        let stamped = self.stamp(points, color);
        if stamped.is_empty() {
            return;
        }
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let width = self.doc.width();
        let Ok(cel) = self.doc.cel(frame, layer) else {
            return;
        };
        let mut changes = Vec::new();
        for (x, y, px) in stamped {
            let i = (y as u32 * width + x as u32) as usize;
            if cel.get(i).copied() != Some(px) {
                changes.push((i, px));
            }
        }
        if changes.is_empty() {
            return;
        }
        if !(coalesce && self.gesture_saved) {
            self.history.push(self.doc.clone());
            if coalesce {
                self.gesture_saved = true;
            }
        }
        if let Ok(cel) = self.doc.cel_mut(frame, layer) {
            for (i, px) in changes {
                cel[i] = px;
            }
        }
        self.dirty = true;
        self.generation += 1;
    }

    fn stamp(&self, points: &[(i32, i32)], color: Option<Rgba>) -> Vec<(i32, i32, Rgba)> {
        let px_color = color.unwrap_or(Rgba::CLEAR);
        let offsets = brush_offsets(self.brush);
        let width = self.doc.width() as i32;
        let height = self.doc.height() as i32;
        let mut out = Vec::new();
        for (x, y) in points {
            for (dx, dy) in &offsets {
                let px = x + dx;
                let py = y + dy;
                if px < 0 || py < 0 || px >= width || py >= height {
                    continue;
                }
                if self.selection.is_some_and(|sel| !sel.contains(px, py)) {
                    continue;
                }
                out.push((px, py, px_color));
            }
        }
        out
    }

    fn bucket(&mut self, x: i32, y: i32, color: Rgba) {
        if x < 0 || y < 0 || x as u32 >= self.doc.width() || y as u32 >= self.doc.height() {
            return;
        }
        if self.selection.is_some_and(|sel| !sel.contains(x, y)) {
            return;
        }
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let width = self.doc.width();
        let height = self.doc.height();
        let Ok(cel) = self.doc.cel(frame, layer) else {
            return;
        };
        let i = (y as u32 * width + x as u32) as usize;
        if cel.get(i).copied() == Some(color) {
            return;
        }
        let snapshot = self.doc.clone();
        let selection = self.selection;
        let Ok(cel) = self.doc.cel_mut(frame, layer) else {
            return;
        };
        if !flood_fill(cel, width, height, x, y, color, selection) {
            self.doc = snapshot;
            return;
        }
        self.history.push(snapshot);
        self.dirty = true;
        self.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        Editor::new(8, 8).unwrap()
    }

    #[test]
    fn pencil_stroke_undo_and_redo() {
        let mut ed = editor();
        ed.set_primary(Rgba::new(1, 2, 3, 255));
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_move(3, 1, false);
        ed.pointer_up(3, 1, false);
        assert_eq!(
            ed.document().cel(0, 0).unwrap()[1 + 8],
            Rgba::new(1, 2, 3, 255)
        );
        assert!(ed.document().cel(0, 0).unwrap()[3 + 8].a == 255);
        assert!(ed.undo());
        assert!(ed.document().cel(0, 0).unwrap().iter().all(|p| p.a == 0));
        assert!(ed.redo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[1 + 8].r, 1);
        assert!(ed.is_dirty());
    }

    #[test]
    fn redo_is_cleared_by_a_new_stroke() {
        let mut ed = editor();
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        assert!(ed.undo());
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        assert!(!ed.can_redo());
    }

    #[test]
    fn eraser_and_secondary_color() {
        let mut ed = editor();
        ed.set_primary(Rgba::new(9, 0, 0, 255));
        ed.pointer_down(2, 2, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        ed.set_tool(Tool::Eraser);
        ed.pointer_down(2, 2, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        assert_eq!(ed.document().cel(0, 0).unwrap()[2 + 16], Rgba::CLEAR);

        ed.set_tool(Tool::Pencil);
        ed.set_secondary(Rgba::new(0, 4, 0, 128));
        ed.pointer_down(0, 0, MouseButton::Secondary, false);
        ed.pointer_up(0, 0, false);
        assert_eq!(ed.document().cel(0, 0).unwrap()[0], Rgba::new(0, 4, 0, 128));
    }

    #[test]
    fn selection_clips_pencil_and_clear() {
        let mut ed = editor();
        ed.selection = Some(PixelRect::from_corners(0, 0, 0, 0));
        ed.set_primary(Rgba::new(5, 0, 0, 255));
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_move(2, 0, false);
        ed.pointer_up(2, 0, false);
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[0].r, 5);
        assert_eq!(cel[1], Rgba::CLEAR);
        ed.clear_selection_pixels().unwrap();
        assert_eq!(ed.document().cel(0, 0).unwrap()[0], Rgba::CLEAR);
        assert!(ed.clear_selection_pixels().is_err() || ed.selection.is_some());
        ed.deselect();
        assert!(matches!(
            ed.clear_selection_pixels(),
            Err(Error::NoSelection)
        ));
    }

    #[test]
    fn bucket_and_line_constraint() {
        let mut ed = editor();
        ed.set_primary(Rgba::new(0, 0, 7, 255));
        ed.set_tool(Tool::Bucket);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        assert!(ed.document().cel(0, 0).unwrap().iter().all(|p| p.b == 7));
        assert!(ed.undo());

        ed.set_tool(Tool::Line);
        ed.pointer_down(0, 0, MouseButton::Primary, true);
        ed.pointer_move(5, 1, true);
        let preview = ed.preview();
        assert!(preview.iter().all(|(_, y, _)| *y == 0));
        ed.pointer_up(5, 1, true);
        assert_eq!(ed.document().cel(0, 0).unwrap()[5].b, 7);
        assert_eq!(ed.document().cel(0, 0).unwrap()[8].a, 0);
    }

    #[test]
    fn rectangle_outline_does_not_fill() {
        let mut ed = editor();
        ed.set_tool(Tool::Rectangle);
        ed.set_primary(Rgba::new(1, 0, 0, 255));
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(3, 3, false);
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[1 + 8].a, 255);
        assert_eq!(cel[2 + 16], Rgba::CLEAR);
    }

    #[test]
    fn eyedropper_reads_the_composite() {
        let mut ed = editor();
        ed.set_primary(Rgba::new(0, 9, 0, 255));
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        ed.add_layer().unwrap();
        ed.set_primary(Rgba::new(9, 0, 0, 255));
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        ed.set_tool(Tool::Eyedropper);
        ed.set_primary(Rgba::CLEAR);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        assert_eq!(ed.primary(), Rgba::new(9, 0, 0, 255));
        ed.set_layer_visible(false);
        assert!(ed.sample(1, 1, MouseButton::Secondary));
        assert_eq!(ed.secondary().g, 9);
    }

    #[test]
    fn layer_delete_undo_restores_pixels() {
        let mut ed = editor();
        ed.set_primary(Rgba::new(4, 0, 0, 255));
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        ed.add_layer().unwrap();
        ed.delete_layer().unwrap();
        assert_eq!(ed.document().layer_count(), 1);
        assert!(ed.undo());
        assert_eq!(ed.document().layer_count(), 2);
    }

    #[test]
    fn opacity_slider_coalesces_into_one_undo() {
        let mut ed = editor();
        ed.begin_coalesce();
        ed.set_layer_opacity(200);
        ed.set_layer_opacity(100);
        ed.end_coalesce();
        assert_eq!(ed.document().layer(0).unwrap().opacity, 100);
        assert!(ed.undo());
        assert_eq!(ed.document().layer(0).unwrap().opacity, 255);
        assert!(!ed.can_undo());
    }

    #[test]
    fn frames_do_not_share_ink() {
        let mut ed = editor();
        ed.set_primary(Rgba::new(3, 0, 0, 255));
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        ed.add_frame().unwrap();
        assert_eq!(ed.document().cel(1, 0).unwrap()[0], Rgba::CLEAR);
        assert_eq!(ed.document().cel(0, 0).unwrap()[0].r, 3);
    }

    #[test]
    fn huge_coordinates_do_not_panic() {
        let mut ed = editor();
        ed.set_tool(Tool::Line);
        ed.pointer_down(i32::MIN, i32::MIN, MouseButton::Primary, false);
        ed.pointer_move(i32::MAX, i32::MAX, false);
        ed.pointer_up(i32::MAX, i32::MAX, false);
    }
}
