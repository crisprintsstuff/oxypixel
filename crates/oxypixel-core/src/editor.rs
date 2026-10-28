//! Editing session: tools, selection, colors, and undo on top of a [`Document`].

use crate::blend::BlendMode;
use crate::color::{color_ramp, nearest_palette_color, shade_step, Rgba};
use crate::document::Document;
use crate::draw::{
    brush_offsets, clamp_to_margin, constrain_line, constrain_square, ellipse_points, fill_region,
    flood_fill, line_points, outline_points, pixel_perfect_path, rect_on_canvas, rect_points,
    scale_nearest, shadow_points, transform_pixels, turned_rect, wrap_coord, FillLimits, PixelRect,
    Transform,
};
use crate::error::{Error, Result};
use crate::history::{History, Snapshot};
use crate::limits::{clamp_name, MAX_BRUSH, MAX_CUSTOM_BRUSH};
use crate::selection::{self, OutlineEdge, SelectMode};
use crate::tags::FrameTag;

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
    /// Select the area the fill tool would reach, with its tolerance and Global setting.
    Wand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Primary,
    Secondary,
}

#[derive(Clone, Debug)]
enum Gesture {
    None,
    /// Tile mode on a tilemap layer: put tiles in the cells the pointer
    /// crosses, or empty them.
    Tiles {
        erase: bool,
        last: (i32, i32),
    },
    Pencil {
        button: MouseButton,
        last: (i32, i32),
        /// Captured at pointer down. A brush larger than one pixel ignores it.
        perfect: bool,
        /// Pointer samples for a pixel-perfect stroke. Empty for an ordinary stroke.
        trail: Vec<(i32, i32)>,
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
        mode: SelectMode,
    },
    /// Drag the current selection. `base` is the cel with the selected pixels
    /// cleared, so placing `patch` does not leave a copy behind.
    MoveSel {
        anchor: (i32, i32),
        origin: PixelRect,
        mask: Option<Vec<bool>>,
        base: Vec<Rgba>,
        patch: Vec<Rgba>,
    },
}

#[derive(Clone, Debug)]
struct Clipboard {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
    /// Which pixels were selected when copied. `None` pastes the whole rectangle.
    mask: Option<Vec<bool>>,
}

/// Pixels copied from a selection that the pencil and eraser stamp instead
/// of a square. Clear pixels are not part of the shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomBrush {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgba>,
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
    mirror_x: bool,
    mirror_y: bool,
    pixel_perfect: bool,
    wrap: bool,
    snap_palette: bool,
    fill_tolerance: u8,
    fill_global: bool,
    shade: bool,
    dither: bool,
    custom_brush: Option<CustomBrush>,
    /// On a tilemap layer, the pencil, eraser, fill, and eyedropper work on
    /// whole tiles instead of pixels.
    tile_mode: bool,
    /// The tile the pencil puts down in tile mode, with its flip bits.
    selected_tile: u32,
    /// Cel pixels from before the current pixel-perfect stroke.
    pencil_base: Option<Vec<Rgba>>,
    /// Bounding rectangle of the selection. Change it only through [`Self::set_selection`].
    selection: Option<PixelRect>,
    /// Which pixels of `selection` are selected, row by row. `None` is all of them.
    selection_mask: Option<Vec<bool>>,
    /// Outline of the selection, kept with it so the canvas does not trace it every frame.
    selection_edges: Vec<OutlineEdge>,
    select_mode: SelectMode,
    clipboard: Option<Clipboard>,
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
            mirror_x: false,
            mirror_y: false,
            pixel_perfect: false,
            wrap: false,
            snap_palette: false,
            fill_tolerance: 0,
            fill_global: false,
            shade: false,
            dither: false,
            custom_brush: None,
            tile_mode: false,
            selected_tile: 1,
            pencil_base: None,
            selection: None,
            selection_mask: None,
            selection_edges: Vec::new(),
            select_mode: SelectMode::Replace,
            clipboard: None,
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

    /// The document has changes that are not in the user's file.
    ///
    /// Restoring a recovery copy uses this. The pixels already come from that
    /// copy, so this does not push an undo step.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
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

    pub fn mirror_x(&self) -> bool {
        self.mirror_x
    }

    pub fn mirror_y(&self) -> bool {
        self.mirror_y
    }

    pub fn set_mirror_x(&mut self, enabled: bool) {
        self.mirror_x = enabled;
    }

    pub fn set_mirror_y(&mut self, enabled: bool) {
        self.mirror_y = enabled;
    }

    /// Drop the extra corner of a one-pixel pencil stroke. Off until the user turns it on.
    pub fn pixel_perfect(&self) -> bool {
        self.pixel_perfect
    }

    pub fn set_pixel_perfect(&mut self, enabled: bool) {
        self.pixel_perfect = enabled;
    }

    /// Stamp and fill past an edge continue on the opposite edge.
    pub fn wraps(&self) -> bool {
        self.wrap
    }

    pub fn set_wrap(&mut self, enabled: bool) {
        self.wrap = enabled;
    }

    /// Largest per-channel difference the bucket still treats as the clicked color.
    pub fn fill_tolerance(&self) -> u8 {
        self.fill_tolerance
    }

    pub fn set_fill_tolerance(&mut self, tolerance: u8) {
        self.fill_tolerance = tolerance;
    }

    /// The bucket recolors every matching pixel on the cel, not one connected area.
    pub fn fills_globally(&self) -> bool {
        self.fill_global
    }

    pub fn set_fill_global(&mut self, global: bool) {
        self.fill_global = global;
    }

    /// The pencil steps each pixel one swatch along the palette instead of
    /// painting a color. Off until the user turns it on.
    pub fn shades(&self) -> bool {
        self.shade
    }

    pub fn set_shade(&mut self, enabled: bool) {
        self.shade = enabled;
    }

    /// The pencil paints a checkerboard of the button's color and the other
    /// one, fixed to the canvas so strokes line up. Off until the user turns it on.
    pub fn dithers(&self) -> bool {
        self.dither
    }

    pub fn set_dither(&mut self, enabled: bool) {
        self.dither = enabled;
    }

    /// The brush the pencil and eraser stamp instead of a square, if one was taken.
    pub fn custom_brush(&self) -> Option<&CustomBrush> {
        self.custom_brush.as_ref()
    }

    /// Take the selected pixels of the active layer as the pencil and eraser
    /// brush. `Ok(false)` when the selection holds no art, which keeps the
    /// brush as it was.
    pub fn brush_from_selection(&mut self) -> Result<bool> {
        let Some(sel) = self.selection else {
            return Err(Error::NoSelection);
        };
        if self.active_is_group() {
            return Err(Error::Group(
                "a group has no pixels; pick a layer inside it",
            ));
        }
        for (what, value) in [("brush width", sel.width), ("brush height", sel.height)] {
            if value > MAX_CUSTOM_BRUSH {
                return Err(Error::Limit {
                    what,
                    value: u64::from(value),
                    max: u64::from(MAX_CUSTOM_BRUSH),
                });
            }
        }
        let mask = self.selection_mask.clone();
        let Some((_, pixels)) = self.capture_selection(sel, mask.as_deref()) else {
            return Ok(false);
        };
        if pixels.iter().all(|px| px.a == 0) {
            return Ok(false);
        }
        self.custom_brush = Some(CustomBrush {
            width: sel.width,
            height: sel.height,
            pixels,
        });
        Ok(true)
    }

    /// Go back to the square brush.
    pub fn clear_custom_brush(&mut self) {
        self.custom_brush = None;
    }

    /// Whether the tools place whole tiles on a tilemap layer.
    pub fn tile_mode(&self) -> bool {
        self.tile_mode
    }

    pub fn set_tile_mode(&mut self, enabled: bool) {
        self.tile_mode = enabled;
    }

    /// The tile (number and flip bits) the pencil puts down in tile mode.
    pub fn selected_tile(&self) -> u32 {
        self.selected_tile
    }

    pub fn set_selected_tile(&mut self, value: u32) {
        self.selected_tile = value;
    }

    /// Tile mode is on and the active layer is a tilemap layer.
    pub fn placing_tiles(&self) -> bool {
        self.tile_mode && self.doc.layer_tileset(self.doc.active_layer()).is_some()
    }

    /// Add a tilemap layer on top, drawing from `tileset`, or from a new
    /// tileset of `tile_width` × `tile_height` tiles when it is `None`.
    pub fn add_tilemap_layer(
        &mut self,
        tileset: Option<usize>,
        tile_width: u32,
        tile_height: u32,
    ) -> Result<()> {
        self.structural(|doc| {
            let set = match tileset {
                Some(set) => set,
                None => doc.add_tileset("", tile_width, tile_height)?,
            };
            doc.add_tilemap_layer("", set)
        })?;
        self.selected_tile = 1;
        Ok(())
    }

    /// Copy the selected tile into a new tile and select it, as a start for
    /// a variant. One undo step.
    pub fn duplicate_tile(&mut self) -> Result<u32> {
        let set = self
            .doc
            .layer_tileset(self.doc.active_layer())
            .ok_or(Error::Tilemap("pick a tilemap layer first"))?;
        let id = self.selected_tile & crate::tilemap::TILE_ID_MASK;
        let pixels = self
            .doc
            .tileset(set)
            .and_then(|t| t.tile(id))
            .ok_or(Error::Index { what: "tile" })?
            .to_vec();
        let mut new = 0;
        self.structural(|doc| {
            new = doc.add_tile(set, pixels)?;
            Ok(())
        })?;
        self.selected_tile = new | (self.selected_tile & crate::tilemap::TILE_FLIPS);
        Ok(new)
    }

    /// Put `value` in the cells under `points` on the active tilemap layer.
    /// The first change of a gesture records the undo step.
    fn place_tiles(&mut self, points: &[(i32, i32)], value: u32) {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let Some(set) = self
            .doc
            .layer_tileset(layer)
            .and_then(|s| self.doc.tileset(s))
        else {
            return;
        };
        let (tw, th) = (set.tile_width() as i32, set.tile_height() as i32);
        let (w, h) = (self.doc.width() as i32, self.doc.height() as i32);
        let mut cells: Vec<(u32, u32)> = Vec::new();
        for &(x, y) in points {
            let (x, y) = if self.wrap {
                self.wrap_point(x, y)
            } else {
                (x, y)
            };
            if x < 0 || y < 0 || x >= w || y >= h || !self.in_selection(x, y) {
                continue;
            }
            let cell = ((x / tw) as u32, (y / th) as u32);
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        let mut changed = false;
        for (col, row) in cells {
            let current = self
                .doc
                .tile_grid(frame, layer)
                .zip(
                    self.doc
                        .tileset(self.doc.layer_tileset(layer).unwrap_or_default()),
                )
                .map(|(grid, set)| {
                    let (cols, _) = set.grid_size(self.doc.width(), self.doc.height());
                    grid[(row * cols + col) as usize]
                });
            if current == Some(value) {
                continue;
            }
            if !self.gesture_saved {
                self.push_undo();
                self.gesture_saved = true;
            }
            if self.doc.set_tile(frame, layer, col, row, value).is_ok() {
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
            self.bump();
        }
    }

    /// Fill the connected cells that hold the same tile as `(x, y)`.
    fn fill_tiles(&mut self, x: i32, y: i32, value: u32) {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let Some(set) = self
            .doc
            .layer_tileset(layer)
            .and_then(|s| self.doc.tileset(s))
        else {
            return;
        };
        let (tw, th) = (set.tile_width() as i32, set.tile_height() as i32);
        let (cols, rows) = set.grid_size(self.doc.width(), self.doc.height());
        let Some(grid) = self.doc.tile_grid(frame, layer) else {
            return;
        };
        if x < 0 || y < 0 || x >= self.doc.width() as i32 || y >= self.doc.height() as i32 {
            return;
        }
        let start = ((x / tw) as u32, (y / th) as u32);
        let target = grid[(start.1 * cols + start.0) as usize];
        if target == value {
            return;
        }
        let mut seen = vec![false; grid.len()];
        let mut stack = vec![start];
        let mut cells = Vec::new();
        while let Some((c, r)) = stack.pop() {
            let i = (r * cols + c) as usize;
            if seen[i] || grid[i] != target {
                continue;
            }
            seen[i] = true;
            cells.push(((c as i32) * tw, (r as i32) * th));
            if c > 0 {
                stack.push((c - 1, r));
            }
            if r > 0 {
                stack.push((c, r - 1));
            }
            if c + 1 < cols {
                stack.push((c + 1, r));
            }
            if r + 1 < rows {
                stack.push((c, r + 1));
            }
        }
        self.gesture_saved = false;
        self.place_tiles(&cells, value);
        self.gesture_saved = false;
    }

    /// The tile under `(x, y)` on the active tilemap layer, with its flips.
    pub fn tile_at(&self, x: i32, y: i32) -> Option<u32> {
        let layer = self.doc.active_layer();
        let set = self.doc.tileset(self.doc.layer_tileset(layer)?)?;
        if x < 0 || y < 0 || x >= self.doc.width() as i32 || y >= self.doc.height() as i32 {
            return None;
        }
        let (cols, _) = set.grid_size(self.doc.width(), self.doc.height());
        let (col, row) = (x as u32 / set.tile_width(), y as u32 / set.tile_height());
        self.doc
            .tile_grid(self.doc.active_frame(), layer)?
            .get((row * cols + col) as usize)
            .copied()
    }

    /// Pencil, eraser, fill, and eyedropper in tile mode. `true` when the
    /// pointer press was a tile action.
    fn tile_pointer_down(&mut self, x: i32, y: i32, button: MouseButton) -> bool {
        if !self.placing_tiles() {
            return false;
        }
        let erase = self.tool == Tool::Eraser || button == MouseButton::Secondary;
        let value = if erase { 0 } else { self.selected_tile };
        match self.tool {
            Tool::Pencil | Tool::Eraser => {
                self.gesture = Gesture::Tiles {
                    erase,
                    last: (x, y),
                };
                self.place_tiles(&[(x, y)], value);
                true
            }
            Tool::Bucket => {
                self.fill_tiles(x, y, value);
                true
            }
            Tool::Eyedropper => {
                if let Some(tile) = self.tile_at(x, y).filter(|&t| t != 0) {
                    self.selected_tile = tile;
                }
                true
            }
            _ => false,
        }
    }

    /// Pencil, fill, and shapes stamp the nearest palette color. Off until the user turns it on.
    pub fn snaps_to_palette(&self) -> bool {
        self.snap_palette
    }

    pub fn set_snap_to_palette(&mut self, enabled: bool) {
        self.snap_palette = enabled;
    }

    /// The canvas pixel a tiled-view coordinate paints, when wrap is on.
    pub fn wrap_point(&self, x: i32, y: i32) -> (i32, i32) {
        (
            wrap_coord(x, self.doc.width() as i32),
            wrap_coord(y, self.doc.height() as i32),
        )
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
        let Some(previous) = self.history.undo(self.snapshot()) else {
            return false;
        };
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.cancel_gesture();
        let Some(next) = self.history.redo(self.snapshot()) else {
            return false;
        };
        self.restore(next);
        true
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            doc: self.doc.clone(),
            selection: self.selection,
            selection_mask: self.selection_mask.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.doc = snapshot.doc;
        self.set_selection(snapshot.selection, snapshot.selection_mask);
        self.dirty = true;
        self.bump();
    }

    fn push_undo(&mut self) {
        self.history.push(self.snapshot());
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

    /// A slider drag is grouping edits into one undo step.
    pub fn coalesce_active(&self) -> bool {
        self.coalescing
    }

    /// Bounding rectangle of the selection.
    pub fn selection(&self) -> Option<PixelRect> {
        self.selection
    }

    /// Whether the selection is not a plain rectangle.
    pub fn selection_is_masked(&self) -> bool {
        self.selection_mask.is_some()
    }

    /// Whether `(x, y)` is selected. False with no selection.
    pub fn is_selected(&self, x: i32, y: i32) -> bool {
        self.selection
            .is_some_and(|sel| selection::mask_contains(sel, self.selection_mask.as_deref(), x, y))
    }

    /// Whether painting may touch `(x, y)`: it is selected, or nothing is.
    fn in_selection(&self, x: i32, y: i32) -> bool {
        self.selection.is_none() || self.is_selected(x, y)
    }

    /// The rectangle to draw. While a marquee is being dragged this is the drag, not the old selection.
    pub fn selection_preview(&self) -> Option<PixelRect> {
        match self.gesture {
            Gesture::Select {
                origin, current, ..
            } => Some(PixelRect::from_corners(
                origin.0, origin.1, current.0, current.1,
            )),
            _ => self.selection,
        }
    }

    /// The marquee being dragged and how it will combine, if one is.
    pub fn marquee_drag(&self) -> Option<(PixelRect, SelectMode)> {
        match self.gesture {
            Gesture::Select {
                origin,
                current,
                mode,
            } => Some((
                PixelRect::from_corners(origin.0, origin.1, current.0, current.1),
                mode,
            )),
            _ => None,
        }
    }

    /// Edges between selected and unselected pixels of the current selection.
    pub fn selection_outline(&self) -> &[OutlineEdge] {
        &self.selection_edges
    }

    /// How the next marquee or wand click combines with the selection. The canvas
    /// sets this from the modifier keys: Shift adds, Alt subtracts.
    pub fn select_mode(&self) -> SelectMode {
        self.select_mode
    }

    pub fn set_select_mode(&mut self, mode: SelectMode) {
        self.select_mode = mode;
    }

    /// The one place the selection changes, so the mask and outline stay with it.
    fn set_selection(&mut self, rect: Option<PixelRect>, mask: Option<Vec<bool>>) {
        let normalized = rect.and_then(|rect| selection::normalize(rect, mask));
        let (rect, mask) = match normalized {
            Some((rect, mask)) => (Some(rect), mask),
            None => (None, None),
        };
        self.selection_edges = match rect {
            Some(rect) => selection::outline(rect, mask.as_deref()),
            None => Vec::new(),
        };
        self.selection = rect;
        self.selection_mask = mask;
    }

    /// Combine a canvas-sized area with the selection by `mode`.
    fn combine_selection(&mut self, area: Vec<bool>, mode: SelectMode) {
        let (w, h) = (self.doc.width(), self.doc.height());
        let current = self
            .selection
            .map(|sel| (sel, self.selection_mask.as_deref()));
        match selection::combine(current, area, mode, w, h) {
            Some((rect, mask)) => self.set_selection(Some(rect), mask),
            None => self.set_selection(None, None),
        }
    }

    pub fn select_all(&mut self) {
        self.cancel_gesture();
        self.set_selection(
            Some(PixelRect {
                x: 0,
                y: 0,
                width: self.doc.width(),
                height: self.doc.height(),
            }),
            None,
        );
    }

    pub fn deselect(&mut self) {
        if matches!(self.gesture, Gesture::Select { .. }) {
            self.gesture = Gesture::None;
        }
        self.set_selection(None, None);
    }

    pub fn cancel_gesture(&mut self) {
        self.gesture = Gesture::None;
        self.gesture_saved = false;
        self.pencil_base = None;
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

    /// Whether the active layer is a group, which has no pixels to draw on.
    pub fn active_is_group(&self) -> bool {
        self.doc
            .layer(self.doc.active_layer())
            .is_some_and(|layer| layer.group)
    }

    pub fn pointer_down(&mut self, x: i32, y: i32, button: MouseButton, shift: bool) {
        let paints = matches!(
            self.tool,
            Tool::Pencil
                | Tool::Eraser
                | Tool::Bucket
                | Tool::Line
                | Tool::Rectangle
                | Tool::Ellipse
        );
        if paints && self.active_is_group() {
            return;
        }
        let (x, y) = self.clamp_pointer(x, y);
        if self.tile_pointer_down(x, y, button) {
            return;
        }
        match self.tool {
            Tool::Pencil | Tool::Eraser => {
                // Shading rewrites the stroke from the pre-stroke cel, the same way
                // pixel-perfect does, so going back over a pixel shades it once.
                let perfect = self.tool == Tool::Pencil && (self.pixel_perfect || self.shade);
                self.gesture = Gesture::Pencil {
                    button,
                    last: (x, y),
                    perfect,
                    trail: if perfect { vec![(x, y)] } else { Vec::new() },
                };
                let color = self.stroke_color(button);
                if perfect {
                    self.paint_pixel_perfect(&[(x, y)], button);
                } else {
                    self.paint(&[(x, y)], color, true);
                }
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
                let mode = self.select_mode;
                if mode == SelectMode::Replace && self.is_selected(x, y) {
                    if let Some(origin) = self.selection {
                        let mask = self.selection_mask.clone();
                        if let Some((base, patch)) = self.capture_selection(origin, mask.as_deref())
                        {
                            self.gesture = Gesture::MoveSel {
                                anchor: (x, y),
                                origin,
                                mask,
                                base,
                                patch,
                            };
                            return;
                        }
                    }
                }
                self.gesture = Gesture::Select {
                    origin: (x, y),
                    current: (x, y),
                    mode,
                };
            }
            Tool::Wand => self.wand(x, y),
        }
    }

    pub fn pointer_move(&mut self, x: i32, y: i32, shift: bool) {
        let (x, y) = self.clamp_pointer(x, y);
        match self.gesture {
            Gesture::Tiles { erase, last } => {
                if (x, y) != last {
                    let value = if erase { 0 } else { self.selected_tile };
                    self.place_tiles(&line_points(last.0, last.1, x, y), value);
                    self.gesture = Gesture::Tiles {
                        erase,
                        last: (x, y),
                    };
                }
            }
            Gesture::Pencil { .. } => {
                let Gesture::Pencil {
                    button,
                    last,
                    perfect,
                    mut trail,
                } = std::mem::replace(&mut self.gesture, Gesture::None)
                else {
                    return;
                };
                let color = self.stroke_color(button);
                if perfect {
                    if trail.last() != Some(&(x, y)) {
                        trail.push((x, y));
                    }
                    self.paint_pixel_perfect(&trail, button);
                } else if (x, y) != last {
                    let points = line_points(last.0, last.1, x, y);
                    self.paint(&points, color, true);
                }
                self.gesture = Gesture::Pencil {
                    button,
                    last: (x, y),
                    perfect,
                    trail,
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
            Gesture::Select { origin, mode, .. } => {
                self.gesture = Gesture::Select {
                    origin,
                    current: (x, y),
                    mode,
                };
            }
            Gesture::MoveSel { .. } => {
                let Gesture::MoveSel {
                    anchor,
                    origin,
                    mask,
                    base,
                    patch,
                } = std::mem::replace(&mut self.gesture, Gesture::None)
                else {
                    return;
                };
                let dx = x - anchor.0;
                let dy = y - anchor.1;
                if dx != 0 || dy != 0 {
                    if !self.gesture_saved {
                        self.push_undo();
                        self.gesture_saved = true;
                    }
                    self.place_moved_selection(origin, mask.as_deref(), &base, &patch, dx, dy);
                }
                self.gesture = Gesture::MoveSel {
                    anchor,
                    origin,
                    mask,
                    base,
                    patch,
                };
            }
            Gesture::None => {}
        }
    }

    pub fn pointer_up(&mut self, x: i32, y: i32, shift: bool) {
        let (x, y) = self.clamp_pointer(x, y);
        match self.gesture {
            Gesture::Tiles { .. } => {
                self.gesture = Gesture::None;
                self.gesture_saved = false;
            }
            Gesture::Pencil { .. } => {
                self.gesture = Gesture::None;
                self.gesture_saved = false;
                self.pencil_base = None;
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
            Gesture::Select { origin, mode, .. } => {
                let rect = PixelRect::from_corners(origin.0, origin.1, x, y);
                self.gesture = Gesture::None;
                if mode == SelectMode::Replace {
                    self.set_selection(Some(rect), None);
                } else {
                    let (w, h) = (self.doc.width(), self.doc.height());
                    self.combine_selection(selection::to_canvas(rect, None, w, h), mode);
                }
            }
            Gesture::MoveSel { .. } => {
                self.gesture = Gesture::None;
                self.gesture_saved = false;
            }
            Gesture::None => {}
        }
    }

    pub fn copy_selection(&mut self) -> bool {
        let Some(sel) = self.selection else {
            return false;
        };
        let Some((_, patch)) = self.capture_selection(sel, self.selection_mask.as_deref()) else {
            return false;
        };
        self.clipboard = Some(Clipboard {
            width: sel.width,
            height: sel.height,
            pixels: patch,
            mask: self.selection_mask.clone(),
        });
        true
    }

    pub fn paste_clipboard(&mut self) -> Result<()> {
        let Some(clip) = self.clipboard.clone() else {
            return Err(Error::EmptyClipboard);
        };
        let x = self.selection.map(|sel| sel.x).unwrap_or(0);
        let y = self.selection.map(|sel| sel.y).unwrap_or(0);
        self.cancel_gesture();
        self.push_undo();
        self.blit_patch(
            x,
            y,
            clip.width,
            clip.height,
            &clip.pixels,
            clip.mask.as_deref(),
        );
        self.set_selection(
            Some(PixelRect {
                x,
                y,
                width: clip.width,
                height: clip.height,
            }),
            clip.mask,
        );
        self.dirty = true;
        self.bump();
        Ok(())
    }

    /// Straight-alpha RGBA8 bytes of the in-memory clipboard, row-major.
    ///
    /// This does not read the operating-system clipboard.
    pub fn clipboard_rgba(&self) -> Result<(u32, u32, Vec<u8>)> {
        let clip = self.clipboard.as_ref().ok_or(Error::EmptyClipboard)?;
        Ok((
            clip.width,
            clip.height,
            crate::clipboard::pixels_to_rgba(&clip.pixels),
        ))
    }

    /// PNG of the in-memory clipboard. The alpha channel is stored straight.
    pub fn clipboard_png(&self) -> Result<Vec<u8>> {
        let clip = self.clipboard.as_ref().ok_or(Error::EmptyClipboard)?;
        crate::clipboard::encode_png(clip.width, clip.height, &clip.pixels)
    }

    /// Replace the in-memory clipboard from straight-alpha RGBA8.
    ///
    /// Does not paint. A rejected buffer leaves the previous patch in place.
    pub fn set_clipboard_rgba(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
        let pixels = crate::clipboard::pixels_from_rgba(width, height, rgba)?;
        self.clipboard = Some(Clipboard {
            width,
            height,
            pixels,
            mask: None,
        });
        Ok(())
    }

    /// Replace the in-memory clipboard from a PNG. Same rejection rule as
    /// [`Self::set_clipboard_rgba`].
    pub fn set_clipboard_png(&mut self, bytes: &[u8]) -> Result<()> {
        let (width, height, pixels) = crate::clipboard::decode_png(bytes)?;
        self.clipboard = Some(Clipboard {
            width,
            height,
            pixels,
            mask: None,
        });
        Ok(())
    }

    pub fn nudge_selection(&mut self, dx: i32, dy: i32) -> Result<()> {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        let Some(sel) = self.selection else {
            return Err(Error::NoSelection);
        };
        let mask = self.selection_mask.clone();
        let Some((base, patch)) = self.capture_selection(sel, mask.as_deref()) else {
            return Err(Error::NoSelection);
        };
        self.cancel_gesture();
        self.push_undo();
        self.place_moved_selection(sel, mask.as_deref(), &base, &patch, dx, dy);
        Ok(())
    }

    /// Flip or turn the selection. With no selection, the same command uses every
    /// pixel of the active cel (this layer on this frame).
    ///
    /// Each call is one undo step. Other layers and frames stay as they are, and
    /// the canvas size does not change. Flips and a 180 degree turn keep the
    /// rectangle. A 90 degree turn swaps its width and height and keeps the same
    /// center. If the turned rectangle fits and still touches the cel, it shifts
    /// fully onto the cel. Pixels that still land outside are dropped, and the
    /// selection covers only the part that remains. A turn that misses the cel
    /// changes nothing.
    pub fn transform_pixels(&mut self, op: Transform) -> Result<()> {
        let canvas_w = self.doc.width();
        let canvas_h = self.doc.height();
        let had_selection = self.selection.is_some();
        let sel = self.selection.unwrap_or(PixelRect {
            x: 0,
            y: 0,
            width: canvas_w,
            height: canvas_h,
        });
        let mask = self.selection_mask.clone();
        let Some((base, patch)) = self.capture_selection(sel, mask.as_deref()) else {
            return Err(Error::Index { what: "cel" });
        };
        let (_width, _height, turned) =
            transform_pixels(sel.width, sel.height, &patch, op).ok_or(Error::Limit {
                what: "pixels in a transform",
                value: u64::from(sel.width).saturating_mul(u64::from(sel.height)),
                max: crate::limits::MAX_CEL_PIXELS,
            })?;
        let turned_mask = match &mask {
            Some(mask) => Some(
                selection::transform_mask(sel.width, sel.height, mask, op).ok_or(Error::Limit {
                    what: "pixels in a transform",
                    value: u64::from(sel.width).saturating_mul(u64::from(sel.height)),
                    max: crate::limits::MAX_CEL_PIXELS,
                })?,
            ),
            None => None,
        };
        let dest = turned_rect(sel, canvas_w, canvas_h, op);
        let kept = rect_on_canvas(dest, canvas_w, canvas_h).ok_or(Error::TransformOutside)?;
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let cel_len = self.doc.cel(frame, layer)?.len();
        if cel_len != base.len() {
            return Err(Error::Limit {
                what: "pixels in a transform",
                value: base.len() as u64,
                max: cel_len as u64,
            });
        }
        self.cancel_gesture();
        self.push_undo();
        if let Ok(cel) = self.doc.cel_mut(frame, layer) {
            if cel.len() == base.len() {
                cel.copy_from_slice(&base);
            }
        }
        self.blit_patch(
            dest.x,
            dest.y,
            dest.width,
            dest.height,
            &turned,
            turned_mask.as_deref(),
        );
        if had_selection {
            let kept_mask = turned_mask.map(|mask| selection::crop_mask(dest, &mask, kept));
            self.set_selection(Some(kept), kept_mask);
        } else {
            self.set_selection(None, None);
        }
        self.dirty = true;
        self.bump();
        Ok(())
    }

    pub fn resize_canvas(
        &mut self,
        width: u32,
        height: u32,
        shift_x: i32,
        shift_y: i32,
    ) -> Result<()> {
        if width == self.doc.width() && height == self.doc.height() && shift_x == 0 && shift_y == 0
        {
            return Ok(());
        }
        self.structural(|doc| doc.resize(width, height, shift_x, shift_y))?;
        self.set_selection(None, None);
        Ok(())
    }

    /// Copy the selected pixels out of the active cel. Returns the cel with them
    /// cleared and the copied patch, which is clear where the mask is off.
    fn capture_selection(
        &self,
        sel: PixelRect,
        mask: Option<&[bool]>,
    ) -> Option<(Vec<Rgba>, Vec<Rgba>)> {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let cel = self.doc.cel(frame, layer).ok()?;
        let width = self.doc.width();
        let height = self.doc.height();
        let mut base = cel.to_vec();
        let mut patch = vec![Rgba::CLEAR; (sel.width * sel.height) as usize];
        for y in 0..sel.height as i32 {
            for x in 0..sel.width as i32 {
                let sx = sel.x + x;
                let sy = sel.y + y;
                if sx < 0 || sy < 0 || sx as u32 >= width || sy as u32 >= height {
                    continue;
                }
                if !selection::mask_contains(sel, mask, sx, sy) {
                    continue;
                }
                let src = (sy as u32 * width + sx as u32) as usize;
                patch[(y as u32 * sel.width + x as u32) as usize] = cel[src];
                base[src] = Rgba::CLEAR;
            }
        }
        Some((base, patch))
    }

    fn place_moved_selection(
        &mut self,
        origin: PixelRect,
        mask: Option<&[bool]>,
        base: &[Rgba],
        patch: &[Rgba],
        dx: i32,
        dy: i32,
    ) {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let Ok(cel) = self.doc.cel_mut(frame, layer) else {
            return;
        };
        if cel.len() != base.len() {
            return;
        }
        cel.copy_from_slice(base);
        self.blit_patch(
            origin.x + dx,
            origin.y + dy,
            origin.width,
            origin.height,
            patch,
            mask,
        );
        let moved = PixelRect {
            x: origin.x + dx,
            y: origin.y + dy,
            width: origin.width,
            height: origin.height,
        };
        // A drag places the same shape every step, so shift its outline instead
        // of tracing the mask again.
        match self.selection {
            Some(prev)
                if (prev.width, prev.height) == (moved.width, moved.height)
                    && self.selection_mask.as_deref() == mask =>
            {
                let (sx, sy) = (moved.x - prev.x, moved.y - prev.y);
                for edge in &mut self.selection_edges {
                    edge.x0 += sx;
                    edge.x1 += sx;
                    edge.y0 += sy;
                    edge.y1 += sy;
                }
                self.selection = Some(moved);
            }
            _ => self.set_selection(Some(moved), mask.map(<[bool]>::to_vec)),
        }
        self.dirty = true;
        self.bump();
    }

    /// Write `pixels` at `dest`. Where `mask` is off the cel keeps its pixel.
    fn blit_patch(
        &mut self,
        dest_x: i32,
        dest_y: i32,
        width: u32,
        height: u32,
        pixels: &[Rgba],
        mask: Option<&[bool]>,
    ) {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let canvas_w = self.doc.width();
        let canvas_h = self.doc.height();
        let Ok(cel) = self.doc.cel_mut(frame, layer) else {
            return;
        };
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let dx = dest_x + x;
                let dy = dest_y + y;
                if dx < 0 || dy < 0 || dx as u32 >= canvas_w || dy as u32 >= canvas_h {
                    continue;
                }
                let src = (y as u32 * width + x as u32) as usize;
                let dst = (dy as u32 * canvas_w + dx as u32) as usize;
                if mask.is_some_and(|mask| !mask.get(src).copied().unwrap_or(false)) {
                    continue;
                }
                if let Some(px) = pixels.get(src) {
                    cel[dst] = *px;
                }
            }
        }
    }

    /// Resize the selection (or the whole layer) to `new_w` by `new_h` with
    /// nearest-neighbor sampling, keeping its top-left corner. Pixels that land
    /// off the canvas are dropped. One undo step.
    pub fn scale_selection(&mut self, new_w: u32, new_h: u32) -> Result<()> {
        if self.active_is_group() {
            return Err(Error::Group(
                "a group has no pixels; pick a layer inside it",
            ));
        }
        let canvas_w = self.doc.width();
        let canvas_h = self.doc.height();
        let had_selection = self.selection.is_some();
        let sel = self.selection.unwrap_or(PixelRect {
            x: 0,
            y: 0,
            width: canvas_w,
            height: canvas_h,
        });
        let too_big = Error::Limit {
            what: "pixels in a scaled selection",
            value: u64::from(new_w).saturating_mul(u64::from(new_h)),
            max: crate::limits::MAX_CEL_PIXELS,
        };
        if new_w == 0
            || new_h == 0
            || new_w > 4 * crate::limits::MAX_DIMENSION
            || new_h > 4 * crate::limits::MAX_DIMENSION
        {
            return Err(too_big);
        }
        let mask = self.selection_mask.clone();
        let (base, patch) = self
            .capture_selection(sel, mask.as_deref())
            .ok_or(Error::Index { what: "cel" })?;
        let scaled = scale_nearest(sel.width, sel.height, &patch, new_w, new_h).ok_or(too_big)?;
        let scaled_mask = match &mask {
            Some(mask) => Some(
                scale_nearest(sel.width, sel.height, mask, new_w, new_h).ok_or(Error::Limit {
                    what: "pixels in a scaled selection",
                    value: u64::from(new_w).saturating_mul(u64::from(new_h)),
                    max: crate::limits::MAX_CEL_PIXELS,
                })?,
            ),
            None => None,
        };
        let dest = PixelRect {
            x: sel.x,
            y: sel.y,
            width: new_w,
            height: new_h,
        };
        let kept = rect_on_canvas(dest, canvas_w, canvas_h).ok_or(Error::TransformOutside)?;
        self.cancel_gesture();
        self.push_undo();
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        if let Ok(cel) = self.doc.cel_mut(frame, layer) {
            if cel.len() == base.len() {
                cel.copy_from_slice(&base);
            }
        }
        self.blit_patch(
            dest.x,
            dest.y,
            new_w,
            new_h,
            &scaled,
            scaled_mask.as_deref(),
        );
        if had_selection {
            let kept_mask = scaled_mask.map(|mask| selection::crop_mask(dest, &mask, kept));
            self.set_selection(Some(kept), kept_mask);
        }
        self.dirty = true;
        self.bump();
        Ok(())
    }

    /// Paint `color` on the clear pixels around the art on the active layer,
    /// inside the selection when there is one. `corners` includes diagonal
    /// neighbors. One undo step; `Ok(false)` when there was nothing to outline.
    pub fn outline(&mut self, color: Rgba, corners: bool) -> Result<bool> {
        self.paint_effect(color, |w, h, cel, inside| {
            outline_points(w, h, cel, corners, inside)
        })
    }

    /// Paint `color` behind the art, offset by `(dx, dy)`, on clear pixels of
    /// the active layer. One undo step; `Ok(false)` when nothing changed.
    pub fn drop_shadow(&mut self, color: Rgba, dx: i32, dy: i32) -> Result<bool> {
        if dx == 0 && dy == 0 {
            return Ok(false);
        }
        self.paint_effect(color, |w, h, cel, inside| {
            shadow_points(w, h, cel, dx, dy, inside)
        })
    }

    /// Change every `from` pixel to `to` on the active layer of the active
    /// frame, or on every layer, every frame, or both. Inside the selection
    /// when there is one. One undo step; returns how many pixels changed.
    pub fn replace_color(
        &mut self,
        from: Rgba,
        to: Rgba,
        all_layers: bool,
        all_frames: bool,
    ) -> Result<usize> {
        if !all_layers && self.active_is_group() {
            return Err(Error::Group(
                "a group has no pixels; pick a layer inside it",
            ));
        }
        let frames: Vec<usize> = if all_frames {
            (0..self.doc.frame_count()).collect()
        } else {
            vec![self.doc.active_frame()]
        };
        let layers: Vec<usize> = if all_layers {
            (0..self.doc.layer_count())
                .filter(|&i| self.doc.layer(i).is_some_and(|layer| !layer.group))
                .collect()
        } else {
            vec![self.doc.active_layer()]
        };
        let width = self.doc.width();
        let mut found = Vec::new();
        for &frame in &frames {
            for &layer in &layers {
                let cel = self.doc.cel(frame, layer)?;
                let hits: Vec<usize> = cel
                    .iter()
                    .enumerate()
                    .filter(|&(i, px)| {
                        *px == from
                            && self
                                .in_selection((i as u32 % width) as i32, (i as u32 / width) as i32)
                    })
                    .map(|(i, _)| i)
                    .collect();
                if !hits.is_empty() {
                    found.push((frame, layer, hits));
                }
            }
        }
        let count = found.iter().map(|(_, _, hits)| hits.len()).sum();
        if count == 0 || from == to {
            return Ok(0);
        }
        self.mutate(false, |doc| {
            for (frame, layer, hits) in found {
                if let Ok(cel) = doc.cel_mut(frame, layer) {
                    for i in hits {
                        cel[i] = to;
                    }
                }
            }
        });
        Ok(count)
    }

    fn paint_effect(
        &mut self,
        color: Rgba,
        points: impl FnOnce(u32, u32, &[Rgba], &dyn Fn(i32, i32) -> bool) -> Vec<(i32, i32)>,
    ) -> Result<bool> {
        if self.active_is_group() {
            return Err(Error::Group(
                "a group has no pixels; pick a layer inside it",
            ));
        }
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let (w, h) = (self.doc.width(), self.doc.height());
        let selected = self.selection.is_some();
        let inside = |x: i32, y: i32| !selected || self.is_selected(x, y);
        let found = points(w, h, self.doc.cel(frame, layer)?, &inside);
        if found.is_empty() {
            return Ok(false);
        }
        self.mutate(false, |doc| {
            if let Ok(cel) = doc.cel_mut(frame, layer) {
                for (x, y) in found {
                    cel[(y as u32 * w + x as u32) as usize] = color;
                }
            }
        });
        Ok(true)
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
                if self.is_selected(x, y) {
                    points.push((x, y));
                }
            }
        }
        self.paint(&points, None, false);
        Ok(())
    }

    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.history.clear();
        self.set_selection(None, None);
        self.cancel_gesture();
        self.end_coalesce();
        self.dirty = false;
        self.bump();
    }

    /// Take an edited copy of the document, as from an AI assistant over the
    /// live bridge, as one undo step. The active frame and layer stay where
    /// they were when they still exist, and a selection that no longer fits
    /// is dropped.
    pub fn apply_external(&mut self, mut doc: Document) {
        self.cancel_gesture();
        self.end_coalesce();
        self.push_undo();
        if doc.frame_count() == self.doc.frame_count()
            && doc.layer_count() == self.doc.layer_count()
        {
            let _ = doc.set_active_frame(self.doc.active_frame());
            let _ = doc.set_active_layer(self.doc.active_layer());
        }
        if (doc.width(), doc.height()) != (self.doc.width(), self.doc.height()) {
            self.set_selection(None, None);
        }
        self.doc = doc;
        self.dirty = true;
        self.bump();
    }

    pub fn add_layer(&mut self) -> Result<()> {
        let n = self.doc.layer_count() + 1;
        self.structural(|doc| doc.add_layer(&format!("Layer {n}")))
    }

    /// Add an empty group on top. One undo step.
    pub fn add_group(&mut self) -> Result<()> {
        self.structural(|doc| doc.add_group(""))
    }

    /// Put the active layer in a new group of its own. One undo step.
    pub fn group_layer(&mut self) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.wrap_in_group(index, ""))
    }

    /// Move the active layer into the group just below it. One undo step.
    pub fn indent_layer(&mut self) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.indent_layer(index))
    }

    /// Move the active layer out of its group. One undo step.
    pub fn outdent_layer(&mut self) -> Result<()> {
        let index = self.doc.active_layer();
        self.structural(|doc| doc.outdent_layer(index))
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

    /// Add an animation tag. One undo step. Returns its index.
    pub fn add_tag(&mut self, tag: FrameTag) -> Result<usize> {
        let index = self.doc.tags().len();
        self.structural(|doc| doc.add_tag(tag).map(|_| ()))?;
        Ok(index)
    }

    /// Replace a tag. One undo step; nothing is recorded when it is unchanged.
    pub fn set_tag(&mut self, index: usize, tag: FrameTag) -> Result<()> {
        if self.doc.tags().get(index) == Some(&tag) {
            return Ok(());
        }
        self.structural(|doc| doc.set_tag(index, tag))
    }

    pub fn remove_tag(&mut self, index: usize) -> Result<()> {
        self.structural(|doc| doc.remove_tag(index))
    }

    /// Name the active frame, as typed. Nothing is recorded when the name does
    /// not change; inside [`Self::begin_coalesce`] typing is one undo step.
    pub fn rename_frame(&mut self, name: &str) {
        let index = self.doc.active_frame();
        let name = clamp_name(name);
        if self.doc.frames().get(index).map(|f| f.name.as_str()) == Some(name.as_str()) {
            return;
        }
        self.mutate(false, |doc| {
            let _ = doc.set_frame_name(index, &name);
        });
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
        let _ = self.set_layer_visible_at(index, visible);
    }

    /// Show or hide one layer and leave the active layer where it is.
    pub fn set_layer_visible_at(&mut self, index: usize, visible: bool) -> Result<()> {
        if self.doc.layer(index).is_none() {
            return Err(Error::Index { what: "layer" });
        }
        if self.doc.layer(index).map(|l| l.visible) == Some(visible) {
            return Ok(());
        }
        self.mutate(false, |doc| {
            if let Some(layer) = doc.layers_mut().get_mut(index) {
                layer.visible = visible;
            }
        });
        Ok(())
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

    /// Change how the active layer mixes with the layers below. One undo step.
    pub fn set_layer_blend(&mut self, mode: BlendMode) {
        let index = self.doc.active_layer();
        if self.doc.layer(index).map(|l| l.blend) == Some(mode) {
            return;
        }
        self.mutate(false, |doc| {
            if let Some(layer) = doc.layers_mut().get_mut(index) {
                layer.blend = mode;
            }
        });
    }

    pub fn set_active_layer(&mut self, index: usize) -> Result<()> {
        self.cancel_gesture();
        self.doc.set_active_layer(index)?;
        self.bump();
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
        self.bump();
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

    /// Replace the palette and record one undo step.
    pub fn set_palette(&mut self, colors: Vec<Rgba>) -> Result<()> {
        self.structural(|doc| doc.set_palette(colors))
    }

    /// Move one swatch. The same index is not an undo step.
    pub fn move_palette_color(&mut self, from: usize, to: usize) -> Result<()> {
        let len = self.doc.palette().len();
        if from >= len || to >= len {
            return Err(Error::Index {
                what: "palette color",
            });
        }
        if from == to {
            return Ok(());
        }
        self.structural(|doc| doc.move_palette_color(from, to))
    }

    /// Replace the inclusive span between two swatches with a ramp of `steps` colors.
    ///
    /// `steps` counts both ends. The earlier index stays the start color. One undo step.
    pub fn fill_palette_ramp(&mut self, from: usize, to: usize, steps: usize) -> Result<()> {
        if !(2..=crate::limits::MAX_PALETTE).contains(&steps) {
            return Err(Error::Limit {
                what: "ramp steps",
                value: steps as u64,
                max: crate::limits::MAX_PALETTE as u64,
            });
        }
        let len = self.doc.palette().len();
        if from >= len || to >= len || from == to {
            return Err(Error::Index {
                what: "palette color",
            });
        }
        let lo = from.min(to);
        let hi = from.max(to);
        let start = self.doc.palette()[lo];
        let end = self.doc.palette()[hi];
        let Some(ramp) = color_ramp(start, end, steps) else {
            return Err(Error::Limit {
                what: "ramp steps",
                value: steps as u64,
                max: crate::limits::MAX_PALETTE as u64,
            });
        };
        let replaced = hi - lo + 1;
        let new_len = len - replaced + ramp.len();
        if new_len > crate::limits::MAX_PALETTE {
            return Err(Error::Limit {
                what: "palette colors",
                value: new_len as u64,
                max: crate::limits::MAX_PALETTE as u64,
            });
        }
        let mut colors = Vec::with_capacity(new_len);
        colors.extend_from_slice(&self.doc.palette()[..lo]);
        colors.extend(ramp);
        colors.extend_from_slice(&self.doc.palette()[hi + 1..]);
        self.structural(|doc| doc.set_palette(colors))
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
        let snapshot = self.snapshot();
        match f(&mut self.doc) {
            Ok(()) => {
                self.history.push(snapshot);
                self.dirty = true;
                self.bump();
                Ok(())
            }
            Err(err) => {
                self.doc = snapshot.doc;
                self.set_selection(snapshot.selection, snapshot.selection_mask);
                Err(err)
            }
        }
    }

    /// The document changed. Pixel edits on tilemap layers go into their
    /// tiles here, so every editing path keeps the tiles and pixels in step.
    fn bump(&mut self) {
        self.doc.sync_tiles();
        self.generation += 1;
    }

    fn mutate(&mut self, coalesce: bool, f: impl FnOnce(&mut Document)) {
        self.cancel_gesture();
        let grouped = coalesce || self.coalescing;
        if !(grouped && self.coalesce_saved) {
            self.push_undo();
            if grouped {
                self.coalesce_saved = true;
            }
        }
        f(&mut self.doc);
        self.dirty = true;
        self.bump();
    }

    fn clamp_pointer(&self, x: i32, y: i32) -> (i32, i32) {
        let mut margin = self.brush as i32 + 2;
        if let Some(brush) = &self.custom_brush {
            margin = margin.max(brush.width.max(brush.height) as i32 + 2);
        }
        if self.wrap {
            margin = margin
                .max(self.doc.width() as i32)
                .max(self.doc.height() as i32);
        }
        (
            clamp_to_margin(x, self.doc.width() as i32, margin),
            clamp_to_margin(y, self.doc.height() as i32, margin),
        )
    }

    fn button_color(&self, button: MouseButton) -> Rgba {
        let color = match button {
            MouseButton::Primary => self.primary,
            MouseButton::Secondary => self.secondary,
        };
        self.color_for_tool(color)
    }

    /// Color shown under the pointer. Snap applies only to tools that stamp a color.
    pub fn hover_color(&self, secondary: bool) -> Rgba {
        let color = if secondary {
            self.secondary
        } else {
            self.primary
        };
        self.color_for_tool(color)
    }

    fn color_for_tool(&self, color: Rgba) -> Rgba {
        if self.snap_palette && tool_snaps_to_palette(self.tool) {
            nearest_palette_color(color, self.doc.palette())
        } else {
            color
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
            self.push_undo();
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
        self.bump();
    }

    /// One-pixel pencil only. The whole stroke is rewritten from the pre-stroke cel,
    /// so a corner dropped later is one undo step with the pixels that remain.
    fn paint_pixel_perfect(&mut self, samples: &[(i32, i32)], button: MouseButton) {
        let expanded = expand_stroke(samples);
        let cleaned = if self.pixel_perfect && self.brush == 1 {
            pixel_perfect_path(&expanded)
        } else {
            expanded
        };
        self.rewrite_stroke(&cleaned, button);
    }

    fn rewrite_stroke(&mut self, points: &[(i32, i32)], button: MouseButton) {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let width = self.doc.width();
        let stamped = self.stamp(points, self.stroke_color(button));
        let Ok(current_cel) = self.doc.cel(frame, layer) else {
            return;
        };
        let current = current_cel.to_vec();
        let stamped = if self.shade {
            let base = self.pencil_base.as_deref().unwrap_or(&current);
            self.shade_stamped(stamped, base, button)
        } else {
            stamped
        };
        if self.pencil_base.is_none() {
            let mut changes = Vec::new();
            for (x, y, px) in &stamped {
                let i = (*y as u32 * width + *x as u32) as usize;
                if current.get(i).copied() != Some(*px) {
                    changes.push((i, *px));
                }
            }
            if changes.is_empty() {
                return;
            }
            self.pencil_base = Some(current);
            self.push_undo();
            self.gesture_saved = true;
            if let Ok(cel) = self.doc.cel_mut(frame, layer) {
                for (i, px) in changes {
                    cel[i] = px;
                }
            }
            self.dirty = true;
            self.bump();
            return;
        }
        let Some(base) = self.pencil_base.clone() else {
            return;
        };
        let mut next = base;
        for (x, y, px) in stamped {
            let i = (y as u32 * width + x as u32) as usize;
            if let Some(slot) = next.get_mut(i) {
                *slot = px;
            }
        }
        if next == current {
            return;
        }
        if let Ok(cel) = self.doc.cel_mut(frame, layer) {
            cel.copy_from_slice(&next);
        }
        self.dirty = true;
        self.bump();
    }

    /// Each stamped pixel becomes the next swatch after its pre-stroke color
    /// (left button) or the one before it (right button). Pixels with no
    /// shade are dropped, so they keep what they had.
    fn shade_stamped(
        &self,
        stamped: Vec<(i32, i32, Rgba)>,
        base: &[Rgba],
        button: MouseButton,
    ) -> Vec<(i32, i32, Rgba)> {
        let width = self.doc.width();
        let forward = button == MouseButton::Primary;
        stamped
            .into_iter()
            .filter_map(|(x, y, _)| {
                let i = (y as u32 * width + x as u32) as usize;
                let before = *base.get(i)?;
                shade_step(before, self.doc.palette(), forward).map(|shade| (x, y, shade))
            })
            .collect()
    }

    fn stamp(&self, points: &[(i32, i32)], color: Option<Rgba>) -> Vec<(i32, i32, Rgba)> {
        let width = self.doc.width() as i32;
        let height = self.doc.height() as i32;
        let mut out = Vec::new();
        if let Some(brush) = self
            .custom_brush
            .as_ref()
            .filter(|_| matches!(self.tool, Tool::Pencil | Tool::Eraser))
        {
            // The brush's own colors, centered on each point. The eraser
            // clears the brush's shape.
            let (bw, bh) = (brush.width as i32, brush.height as i32);
            for (x, y) in points {
                for by in 0..bh {
                    for bx in 0..bw {
                        let px = brush.pixels[(by * bw + bx) as usize];
                        if px.a == 0 {
                            continue;
                        }
                        let px = if color.is_some() { px } else { Rgba::CLEAR };
                        self.stamp_one(
                            &mut out,
                            x - bw / 2 + bx,
                            y - bh / 2 + by,
                            px,
                            width,
                            height,
                        );
                    }
                }
            }
            return out;
        }
        let px_color = color.unwrap_or(Rgba::CLEAR);
        let offsets = brush_offsets(self.brush);
        for (x, y) in points {
            for (dx, dy) in &offsets {
                let px = x + dx;
                let py = y + dy;
                self.stamp_one(&mut out, px, py, px_color, width, height);
            }
        }
        if let (true, Tool::Pencil, Some(color)) = (self.dither, self.tool, color) {
            // The other button's color fills the odd squares of the checkerboard.
            let primary = self.button_color(MouseButton::Primary);
            let other = if color == primary {
                self.button_color(MouseButton::Secondary)
            } else {
                primary
            };
            for (x, y, px) in &mut out {
                if (*x + *y).rem_euclid(2) == 1 {
                    *px = other;
                }
            }
        }
        out
    }

    fn stamp_one(
        &self,
        out: &mut Vec<(i32, i32, Rgba)>,
        x: i32,
        y: i32,
        color: Rgba,
        width: i32,
        height: i32,
    ) {
        let mut points = vec![(x, y)];
        if self.mirror_x {
            points.push((width - 1 - x, y));
        }
        if self.mirror_y {
            points.push((x, height - 1 - y));
        }
        if self.mirror_x && self.mirror_y {
            points.push((width - 1 - x, height - 1 - y));
        }
        for (mut px, mut py) in points {
            if self.wrap {
                px = wrap_coord(px, width);
                py = wrap_coord(py, height);
            }
            if px < 0 || py < 0 || px >= width || py >= height {
                continue;
            }
            if !self.in_selection(px, py) {
                continue;
            }
            out.push((px, py, color));
        }
    }

    fn bucket(&mut self, x: i32, y: i32, color: Rgba) {
        let (x, y) = if self.wrap {
            self.wrap_point(x, y)
        } else {
            (x, y)
        };
        if x < 0 || y < 0 || x as u32 >= self.doc.width() || y as u32 >= self.doc.height() {
            return;
        }
        if !self.in_selection(x, y) {
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
        if self.fill_tolerance == 0 && cel.get(i).copied() == Some(color) {
            return;
        }
        let snapshot = self.snapshot();
        let Ok(cel) = self.doc.cel_mut(frame, layer) else {
            return;
        };
        if !flood_fill(
            cel,
            width,
            height,
            x,
            y,
            color,
            FillLimits {
                selection: self.selection,
                mask: self.selection_mask.as_deref(),
                wrap: self.wrap,
                tolerance: self.fill_tolerance,
                global: self.fill_global,
            },
        ) {
            self.doc = snapshot.doc;
            return;
        }
        self.history.push(snapshot);
        self.dirty = true;
        self.bump();
    }

    /// Select what the fill tool would reach from `(x, y)` on the active cel,
    /// combined with the selection by the select mode. The current selection does
    /// not limit the search. A click off the cel changes nothing.
    fn wand(&mut self, x: i32, y: i32) {
        let frame = self.doc.active_frame();
        let layer = self.doc.active_layer();
        let Ok(cel) = self.doc.cel(frame, layer) else {
            return;
        };
        let limits = FillLimits {
            wrap: self.wrap,
            tolerance: self.fill_tolerance,
            global: self.fill_global,
            ..Default::default()
        };
        let Some(area) = fill_region(cel, self.doc.width(), self.doc.height(), x, y, limits) else {
            return;
        };
        self.combine_selection(area, self.select_mode);
    }
}

fn tool_snaps_to_palette(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Pencil | Tool::Bucket | Tool::Line | Tool::Rectangle | Tool::Ellipse
    )
}

/// Connect pointer samples with the same line the ordinary pencil uses.
fn expand_stroke(samples: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut expanded = Vec::new();
    let Some(&first) = samples.first() else {
        return expanded;
    };
    let mut prev = first;
    expanded.push(prev);
    for &point in samples.iter().skip(1) {
        if point == prev {
            continue;
        }
        let segment = line_points(prev.0, prev.1, point.0, point.1);
        expanded.extend(segment.into_iter().skip(1));
        prev = point;
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        Editor::new(8, 8).unwrap()
    }

    #[test]
    fn palette_replace_is_one_undo() {
        let mut ed = editor();
        let before = ed.document().palette().to_vec();
        let colors = vec![Rgba::new(9, 8, 7, 255), Rgba::new(1, 2, 3, 128)];
        ed.set_palette(colors.clone()).unwrap();
        assert_eq!(ed.document().palette(), colors.as_slice());
        assert!(ed.is_dirty());
        assert!(ed.undo());
        assert_eq!(ed.document().palette(), before.as_slice());
        assert!(ed.redo());
        assert_eq!(ed.document().palette(), colors.as_slice());

        assert!(ed.set_palette(Vec::new()).is_err());
        assert_eq!(ed.document().palette(), colors.as_slice());
        assert!(matches!(
            ed.set_palette(vec![Rgba::new(0, 0, 0, 255); 257]),
            Err(Error::Limit {
                what: "palette colors",
                ..
            })
        ));
        assert_eq!(ed.document().palette(), colors.as_slice());
        assert!(ed.undo());
        assert_eq!(ed.document().palette(), before.as_slice());
        assert!(!ed.can_undo());
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
    fn groups_are_undoable_and_tools_do_not_draw_on_them() {
        let mut editor = Editor::new(4, 4).unwrap();
        editor.add_group().unwrap();
        assert!(editor.active_is_group());
        let before = editor.document().clone();
        editor.set_tool(Tool::Pencil);
        editor.pointer_down(1, 1, MouseButton::Primary, false);
        editor.pointer_up(1, 1, false);
        assert_eq!(
            editor.document(),
            &before,
            "drawing on a group does nothing"
        );
        editor.add_layer().unwrap();
        editor.indent_layer().unwrap();
        assert_eq!(editor.document().layer(2).unwrap().depth, 1);
        editor.pointer_down(1, 1, MouseButton::Primary, false);
        editor.pointer_up(1, 1, false);
        assert_eq!(
            editor.document().composite_pixel(0, 1, 1).map(|p| p.a),
            Some(255)
        );
        assert!(editor.undo());
        assert!(editor.undo());
        assert_eq!(editor.document().layer(2).unwrap().depth, 0);
        assert!(editor.outdent_layer().is_err());
    }

    #[test]
    fn blend_mode_change_is_one_undo_step() {
        let mut editor = Editor::demo().unwrap();
        editor.set_active_layer(1).unwrap();
        editor.set_layer_blend(BlendMode::Screen);
        editor.set_layer_blend(BlendMode::Screen);
        assert_eq!(editor.document().layer(1).unwrap().blend, BlendMode::Screen);
        assert!(editor.undo());
        assert_eq!(editor.document().layer(1).unwrap().blend, BlendMode::Normal);
        assert!(!editor.can_undo());
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
    fn hiding_another_layer_keeps_the_active_one() {
        let mut ed = editor();
        ed.add_layer().unwrap();
        let active = ed.document().active_layer();
        ed.set_layer_visible_at(0, false).unwrap();
        assert_eq!(ed.document().active_layer(), active);
        assert!(!ed.document().layer(0).unwrap().visible);
        assert!(ed.document().layer(active).unwrap().visible);
        assert!(ed.set_layer_visible_at(4, false).is_err());
    }

    #[test]
    fn moving_a_selection_does_not_leave_a_copy() {
        let mut ed = editor();
        let ink = Rgba::new(9, 0, 0, 255);
        ed.set_primary(ink);
        ed.pointer_down(2, 2, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        ed.set_tool(Tool::Select);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(3, 3, false);
        ed.pointer_down(2, 2, MouseButton::Primary, false);
        ed.pointer_move(4, 3, false);
        ed.pointer_up(4, 3, false);
        let cel = ed.document().cel(0, 0).unwrap();
        let painted = cel.iter().filter(|px| px.a > 0).count();
        assert_eq!(painted, 1);
        assert_eq!(cel[4 + 3 * 8], ink);
        assert_eq!(cel[2 + 2 * 8], Rgba::CLEAR);
        let sel = ed.selection().unwrap();
        assert_eq!((sel.x, sel.y, sel.width, sel.height), (3, 2, 3, 3));
        assert!(ed.undo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[2 + 2 * 8], ink);
        assert_eq!(ed.selection().unwrap().x, 1);
        assert_eq!(ed.selection().unwrap().y, 1);
    }

    #[test]
    fn nudge_copy_and_paste_cross_frames() {
        let mut ed = editor();
        let ink = Rgba::new(4, 5, 6, 255);
        ed.set_primary(ink);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        ed.set_tool(Tool::Select);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        ed.nudge_selection(1, 0).unwrap();
        assert_eq!(ed.document().cel(0, 0).unwrap()[2 + 8], ink);
        assert_eq!(ed.document().cel(0, 0).unwrap()[1 + 8], Rgba::CLEAR);
        assert!(ed.copy_selection());
        ed.add_frame().unwrap();
        ed.paste_clipboard().unwrap();
        assert_eq!(ed.document().cel(1, 0).unwrap()[2 + 8], ink);
        assert_eq!(ed.document().cel(0, 0).unwrap()[2 + 8], ink);
        assert_eq!(ed.selection().unwrap().x, 2);
        assert!(ed.undo());
        assert_eq!(ed.document().cel(1, 0).unwrap()[2 + 8], Rgba::CLEAR);
        assert!(matches!(
            Editor::new(2, 2).unwrap().paste_clipboard(),
            Err(Error::EmptyClipboard)
        ));
    }

    #[test]
    fn mirror_stamps_the_opposite_side_and_respects_selection() {
        let mut ed = editor();
        let ink = Rgba::new(1, 2, 3, 255);
        ed.set_primary(ink);
        ed.set_mirror_x(true);
        ed.set_mirror_y(true);
        ed.pointer_down(1, 2, MouseButton::Primary, false);
        ed.pointer_up(1, 2, false);
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[1 + 2 * 8], ink);
        assert_eq!(cel[6 + 2 * 8], ink);
        assert_eq!(cel[1 + 5 * 8], ink);
        assert_eq!(cel[6 + 5 * 8], ink);
        ed.undo();
        ed.selection = Some(PixelRect {
            x: 0,
            y: 0,
            width: 4,
            height: 8,
        });
        ed.pointer_down(1, 2, MouseButton::Primary, false);
        ed.pointer_up(1, 2, false);
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[1 + 2 * 8], ink);
        assert_eq!(cel[6 + 2 * 8], Rgba::CLEAR);
    }

    #[test]
    fn resize_canvas_is_one_undo_and_drops_the_marquee() {
        let mut ed = editor();
        let ink = Rgba::new(8, 0, 0, 255);
        ed.set_primary(ink);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        ed.select_all();
        ed.resize_canvas(4, 4, 1, 1).unwrap();
        assert_eq!(ed.document().width(), 4);
        assert_eq!(ed.document().cel(0, 0).unwrap()[1 + 4], ink);
        assert!(ed.selection().is_none());
        assert!(ed.undo());
        assert_eq!(ed.document().width(), 8);
        assert_eq!(ed.document().cel(0, 0).unwrap()[0], ink);
        assert_eq!(ed.selection().unwrap().width, 8);
        ed.resize_canvas(8, 8, 0, 0).unwrap();
        assert!(ed.selection().is_some());
    }

    fn ink(n: u8) -> Rgba {
        Rgba::new(n, 10, 20, 255)
    }

    fn paint(ed: &mut Editor, x: i32, y: i32, color: Rgba) {
        ed.doc
            .set_pixel(ed.doc.active_frame(), ed.doc.active_layer(), x, y, color)
            .unwrap();
    }

    #[test]
    fn flip_horizontal_mirrors_the_selection_and_is_one_undo() {
        let mut ed = Editor::new(6, 4).unwrap();
        let faded = Rgba::new(1, 2, 3, 128);
        paint(&mut ed, 1, 1, ink(1));
        paint(&mut ed, 2, 1, faded);
        paint(&mut ed, 1, 2, ink(3));
        paint(&mut ed, 5, 1, ink(9));
        ed.selection = Some(PixelRect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        });
        ed.transform_pixels(Transform::FlipHorizontal).unwrap();
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[1 + 6], faded);
        assert_eq!(cel[2 + 6], ink(1));
        assert_eq!(cel[1 + 12], Rgba::CLEAR);
        assert_eq!(cel[2 + 12], ink(3));
        assert_eq!(cel[5 + 6], ink(9));
        let sel = ed.selection().unwrap();
        assert_eq!((sel.x, sel.y, sel.width, sel.height), (1, 1, 2, 2));
        assert!(ed.undo());
        let restored = ed.document().cel(0, 0).unwrap();
        assert_eq!(restored[1 + 6], ink(1));
        assert_eq!(restored[2 + 6], faded);
        assert_eq!(ed.selection().unwrap().x, 1);
        assert!(!ed.can_undo());
        assert!(ed.redo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[1 + 6], faded);
        assert!(ed.undo());
        assert!(!ed.can_undo());
    }

    #[test]
    fn rotate_90_swaps_a_non_square_selection_around_its_center() {
        let mut ed = Editor::new(8, 8).unwrap();
        let colors = [
            (3, 2, ink(1)),
            (4, 2, ink(2)),
            (3, 3, ink(3)),
            (4, 3, ink(4)),
            (3, 4, ink(5)),
            (4, 4, ink(6)),
        ];
        for (x, y, color) in colors {
            paint(&mut ed, x, y, color);
        }
        paint(&mut ed, 5, 2, ink(8));
        ed.selection = Some(PixelRect {
            x: 3,
            y: 2,
            width: 2,
            height: 3,
        });
        ed.transform_pixels(Transform::Rotate90Cw).unwrap();
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[3 + 2 * 8], ink(5));
        assert_eq!(cel[4 + 2 * 8], ink(3));
        assert_eq!(cel[5 + 2 * 8], ink(1));
        assert_eq!(cel[3 + 3 * 8], ink(6));
        assert_eq!(cel[4 + 3 * 8], ink(4));
        assert_eq!(cel[5 + 3 * 8], ink(2));
        assert_eq!(cel[3 + 4 * 8], Rgba::CLEAR);
        assert_eq!(cel[4 + 4 * 8], Rgba::CLEAR);
        let sel = ed.selection().unwrap();
        assert_eq!((sel.x, sel.y, sel.width, sel.height), (3, 2, 3, 2));
        assert!(ed.undo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[3 + 4 * 8], ink(5));
        assert_eq!(ed.selection().unwrap().height, 3);
    }

    #[test]
    fn rotate_shifts_a_selection_that_would_hang_off_the_edge() {
        let mut ed = Editor::new(8, 8).unwrap();
        paint(&mut ed, 0, 0, ink(1));
        paint(&mut ed, 1, 0, ink(2));
        paint(&mut ed, 0, 3, ink(3));
        ed.selection = Some(PixelRect {
            x: 0,
            y: 0,
            width: 2,
            height: 4,
        });
        ed.transform_pixels(Transform::Rotate90Cw).unwrap();
        let sel = ed.selection().unwrap();
        assert_eq!((sel.x, sel.y, sel.width, sel.height), (0, 1, 4, 2));
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[0], Rgba::CLEAR);
        assert_eq!(cel[8], ink(3));
        assert_eq!(cel[3 + 8], ink(1));
        assert_eq!(cel[3 + 16], ink(2));
    }

    #[test]
    fn turning_the_active_cel_clips_a_non_square_canvas_and_leaves_the_rest() {
        let mut ed = Editor::new(3, 1).unwrap();
        paint(&mut ed, 0, 0, ink(1));
        paint(&mut ed, 1, 0, ink(2));
        paint(&mut ed, 2, 0, ink(3));
        ed.add_frame().unwrap();
        paint(&mut ed, 0, 0, ink(7));
        ed.set_active_frame(0).unwrap();
        ed.add_layer().unwrap();
        ed.set_active_layer(0).unwrap();
        assert!(ed.selection().is_none());
        ed.transform_pixels(Transform::Rotate90Cw).unwrap();
        assert_eq!((ed.document().width(), ed.document().height()), (3, 1));
        assert!(ed.selection().is_none());
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[0], Rgba::CLEAR);
        assert_eq!(cel[1], ink(2));
        assert_eq!(cel[2], Rgba::CLEAR);
        assert_eq!(ed.document().cel(1, 0).unwrap()[0], ink(7));
        assert!(ed.document().cel(0, 1).unwrap().iter().all(|px| px.a == 0));
        ed.transform_pixels(Transform::FlipVertical).unwrap();
        ed.transform_pixels(Transform::Rotate180).unwrap();
        ed.transform_pixels(Transform::Rotate90Ccw).unwrap();
        assert_eq!(ed.document().cel(1, 0).unwrap()[0], ink(7));
        assert!(ed.undo());
        assert!(ed.undo());
        assert!(ed.undo());
        assert!(ed.undo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[0], ink(1));
        assert_eq!(ed.document().cel(0, 0).unwrap()[2], ink(3));
        assert!(ed.can_undo());
    }

    #[test]
    fn a_turn_that_lands_fully_outside_does_not_change_the_cel() {
        let mut ed = editor();
        paint(&mut ed, 1, 1, ink(4));
        ed.selection = Some(PixelRect {
            x: 100,
            y: 100,
            width: 2,
            height: 1,
        });
        assert!(matches!(
            ed.transform_pixels(Transform::Rotate90Cw),
            Err(Error::TransformOutside)
        ));
        assert!(!ed.can_undo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[1 + 8], ink(4));
        assert_eq!(ed.selection().unwrap().x, 100);
    }

    #[test]
    fn huge_coordinates_do_not_panic() {
        let mut ed = editor();
        ed.set_tool(Tool::Line);
        ed.pointer_down(i32::MIN, i32::MIN, MouseButton::Primary, false);
        ed.pointer_move(i32::MAX, i32::MAX, false);
        ed.pointer_up(i32::MAX, i32::MAX, false);
        ed.set_wrap(true);
        ed.set_tool(Tool::Pencil);
        ed.pointer_down(i32::MIN, 0, MouseButton::Primary, false);
        ed.pointer_move(i32::MAX, i32::MAX, false);
        ed.pointer_up(i32::MAX, 0, false);
    }

    fn cel_pixel(ed: &Editor, x: i32, y: i32) -> Rgba {
        let width = ed.document().width();
        ed.document().cel(0, 0).unwrap()[(y as u32 * width + x as u32) as usize]
    }

    #[test]
    fn pixel_perfect_pencil_drops_the_corner_and_is_one_undo() {
        let mut ed = editor();
        let ink = Rgba::new(4, 5, 6, 255);
        ed.set_primary(ink);
        ed.set_pixel_perfect(true);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_move(1, 0, false);
        ed.pointer_move(1, 1, false);
        ed.pointer_up(1, 1, false);
        assert_eq!(cel_pixel(&ed, 0, 0), ink);
        assert_eq!(cel_pixel(&ed, 1, 0), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 1, 1), ink);
        assert!(ed.undo());
        assert!(ed
            .document()
            .cel(0, 0)
            .unwrap()
            .iter()
            .all(|px| *px == Rgba::CLEAR));
        assert!(!ed.can_undo());
        assert!(ed.redo());
        assert_eq!(cel_pixel(&ed, 1, 1), ink);
        assert_eq!(cel_pixel(&ed, 1, 0), Rgba::CLEAR);

        let old = Rgba::new(9, 0, 0, 255);
        paint(&mut ed, 1, 0, old);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_move(1, 0, false);
        ed.pointer_move(1, 1, false);
        ed.pointer_up(1, 1, false);
        assert_eq!(cel_pixel(&ed, 1, 0), old);
        assert_eq!(cel_pixel(&ed, 0, 0), ink);
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 1, 0), old);
        assert_eq!(cel_pixel(&ed, 0, 0), ink);
        assert_eq!(cel_pixel(&ed, 1, 1), ink);
    }

    #[test]
    fn pixel_perfect_is_off_for_a_wide_brush_and_the_line_tool() {
        let mut ed = editor();
        let ink = Rgba::new(8, 0, 0, 255);
        ed.set_primary(ink);
        ed.set_pixel_perfect(true);
        ed.set_brush(2);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_move(2, 1, false);
        ed.pointer_move(2, 2, false);
        ed.pointer_up(2, 2, false);
        assert_eq!(cel_pixel(&ed, 2, 0), ink);

        let mut line = editor();
        line.set_primary(ink);
        line.set_pixel_perfect(true);
        line.set_tool(Tool::Line);
        line.pointer_down(0, 0, MouseButton::Primary, false);
        line.pointer_move(3, 1, false);
        line.pointer_up(3, 1, false);
        let mut plain = editor();
        plain.set_primary(ink);
        plain.set_tool(Tool::Line);
        plain.pointer_down(0, 0, MouseButton::Primary, false);
        plain.pointer_move(3, 1, false);
        plain.pointer_up(3, 1, false);
        assert_eq!(
            line.document().cel(0, 0).unwrap(),
            plain.document().cel(0, 0).unwrap()
        );
    }

    #[test]
    fn pixel_perfect_mirror_drops_the_corner_on_both_sides() {
        let mut ed = editor();
        let ink = Rgba::new(1, 2, 3, 255);
        ed.set_primary(ink);
        ed.set_pixel_perfect(true);
        ed.set_mirror_x(true);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_move(2, 1, false);
        ed.pointer_move(2, 2, false);
        ed.pointer_up(2, 2, false);
        assert_eq!(cel_pixel(&ed, 1, 1), ink);
        assert_eq!(cel_pixel(&ed, 2, 1), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 2, 2), ink);
        assert_eq!(cel_pixel(&ed, 6, 1), ink);
        assert_eq!(cel_pixel(&ed, 5, 1), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 5, 2), ink);
        assert!(ed.undo());
        assert!(ed.document().cel(0, 0).unwrap().iter().all(|px| px.a == 0));
    }

    #[test]
    fn wrap_continues_a_stroke_fill_and_preview_inside_the_same_cel() {
        let mut ed = editor();
        let ink = Rgba::new(9, 1, 2, 255);
        ed.set_primary(ink);
        ed.set_wrap(true);
        ed.pointer_down(0, 1, MouseButton::Primary, false);
        ed.pointer_move(-1, 1, false);
        ed.pointer_up(-1, 1, false);
        assert_eq!(cel_pixel(&ed, 0, 1), ink);
        assert_eq!(cel_pixel(&ed, 7, 1), ink);
        assert_eq!(ed.document().cel(0, 0).unwrap().len(), 64);

        ed.set_tool(Tool::Line);
        ed.pointer_down(0, 3, MouseButton::Primary, false);
        ed.pointer_move(-1, 3, false);
        let preview = ed.preview();
        assert!(preview.iter().any(|(x, y, _)| *x == 7 && *y == 3));
        ed.pointer_up(-1, 3, false);
        assert_eq!(cel_pixel(&ed, 7, 3), ink);

        ed.set_tool(Tool::Rectangle);
        ed.pointer_down(0, 4, MouseButton::Primary, false);
        ed.pointer_move(-1, 4, false);
        ed.pointer_up(-1, 4, false);
        assert_eq!(cel_pixel(&ed, 0, 4), ink);
        assert_eq!(cel_pixel(&ed, 7, 4), ink);

        ed.set_tool(Tool::Eraser);
        ed.pointer_down(0, 4, MouseButton::Primary, false);
        ed.pointer_move(-1, 4, false);
        ed.pointer_up(-1, 4, false);
        assert_eq!(cel_pixel(&ed, 0, 4), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 7, 4), Rgba::CLEAR);

        let mut tile = Editor::new(4, 1).unwrap();
        tile.set_wrap(true);
        tile.set_primary(ink);
        paint(&mut tile, 1, 0, Rgba::new(1, 0, 0, 255));
        tile.set_tool(Tool::Bucket);
        tile.pointer_down(0, 0, MouseButton::Primary, false);
        tile.pointer_up(0, 0, false);
        assert_eq!(cel_pixel(&tile, 0, 0), ink);
        assert_eq!(cel_pixel(&tile, 1, 0), Rgba::new(1, 0, 0, 255));
        assert_eq!(cel_pixel(&tile, 2, 0), ink);
        assert_eq!(cel_pixel(&tile, 3, 0), ink);
        assert_eq!(tile.document().width(), 4);
    }

    #[test]
    fn bucket_tolerance_and_global_are_one_undo_step() {
        let mut ed = Editor::new(3, 1).unwrap();
        let base = Rgba::new(50, 50, 50, 255);
        let near = Rgba::new(54, 50, 50, 255);
        let wall = Rgba::new(250, 0, 0, 255);
        ed.set_tool(Tool::Pencil);
        for (x, color) in [(0, base), (1, wall), (2, near)] {
            ed.set_primary(color);
            ed.pointer_down(x, 0, MouseButton::Primary, false);
            ed.pointer_up(x, 0, false);
        }
        let ink = Rgba::new(0, 0, 255, 255);
        ed.set_primary(ink);
        ed.set_tool(Tool::Bucket);
        assert_eq!(ed.fill_tolerance(), 0);
        assert!(!ed.fills_globally());

        ed.set_fill_global(true);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        assert_eq!(cel_pixel(&ed, 0, 0), ink);
        assert_eq!(cel_pixel(&ed, 2, 0), near);
        assert!(ed.undo());

        ed.set_fill_tolerance(4);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        assert_eq!(cel_pixel(&ed, 0, 0), ink);
        assert_eq!(cel_pixel(&ed, 1, 0), wall);
        assert_eq!(cel_pixel(&ed, 2, 0), ink);
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 0, 0), base);
        assert_eq!(cel_pixel(&ed, 2, 0), near);
    }

    #[test]
    fn shading_steps_each_pixel_once_along_the_palette() {
        let dark = Rgba::new(10, 10, 10, 255);
        let mid = Rgba::new(80, 80, 80, 255);
        let light = Rgba::new(200, 200, 200, 255);
        let other = Rgba::new(1, 200, 3, 255);
        let mut ed = Editor::new(4, 1).unwrap();
        ed.set_palette(vec![dark, mid, light]).unwrap();
        for (x, color) in [(0, dark), (1, dark), (2, mid), (3, other)] {
            ed.set_primary(color);
            ed.pointer_down(x, 0, MouseButton::Primary, false);
            ed.pointer_up(x, 0, false);
        }
        ed.set_shade(true);
        assert!(ed.shades());

        // Across the row and back again: each pixel moves one swatch, once.
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_move(3, 0, false);
        ed.pointer_move(0, 0, false);
        ed.pointer_up(0, 0, false);
        assert_eq!(cel_pixel(&ed, 0, 0), mid);
        assert_eq!(cel_pixel(&ed, 1, 0), mid);
        assert_eq!(cel_pixel(&ed, 2, 0), light);
        assert_eq!(cel_pixel(&ed, 3, 0), other);

        // A second stroke steps again; the lightest swatch stays.
        ed.pointer_down(2, 0, MouseButton::Primary, false);
        ed.pointer_up(2, 0, false);
        assert_eq!(cel_pixel(&ed, 2, 0), light);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        assert_eq!(cel_pixel(&ed, 0, 0), light);

        // The right button steps back.
        ed.pointer_down(1, 0, MouseButton::Secondary, false);
        ed.pointer_up(1, 0, false);
        assert_eq!(cel_pixel(&ed, 1, 0), dark);

        // Each stroke was one undo step.
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 1, 0), mid);
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 0, 0), mid);
        assert!(ed.undo());
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 0, 0), dark);
        assert_eq!(cel_pixel(&ed, 2, 0), mid);
    }

    #[test]
    fn shading_follows_the_mirror_and_leaves_the_eraser_alone() {
        let dark = Rgba::new(10, 10, 10, 255);
        let mid = Rgba::new(80, 80, 80, 255);
        let mut ed = Editor::new(4, 1).unwrap();
        ed.set_palette(vec![dark, mid]).unwrap();
        for x in 0..4 {
            ed.set_primary(dark);
            ed.pointer_down(x, 0, MouseButton::Primary, false);
            ed.pointer_up(x, 0, false);
        }
        ed.set_shade(true);
        ed.set_mirror_x(true);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        assert_eq!(cel_pixel(&ed, 0, 0), mid);
        assert_eq!(cel_pixel(&ed, 3, 0), mid);
        assert_eq!(cel_pixel(&ed, 1, 0), dark);

        ed.set_tool(Tool::Eraser);
        ed.pointer_down(1, 0, MouseButton::Primary, false);
        ed.pointer_up(1, 0, false);
        assert_eq!(cel_pixel(&ed, 1, 0), Rgba::CLEAR);
    }

    #[test]
    fn moving_a_swatch_is_one_undo_and_exports_in_the_new_order() {
        let mut ed = Editor::new(4, 4).unwrap();
        let colors = vec![
            Rgba::new(10, 0, 0, 255),
            Rgba::new(0, 20, 0, 255),
            Rgba::new(0, 0, 30, 128),
            Rgba::new(40, 50, 60, 255),
        ];
        ed.set_palette(colors.clone()).unwrap();
        // Dropping a swatch on itself records nothing: one undo removes the
        // palette itself and leaves no step behind it.
        assert!(ed.move_palette_color(0, 0).is_ok());
        assert!(ed.undo());
        assert!(!ed.can_undo());
        assert_ne!(ed.document().palette(), colors.as_slice());
        assert!(ed.redo());
        assert_eq!(ed.document().palette(), colors.as_slice());
        ed.move_palette_color(0, 3).unwrap();
        let moved = ed.document().palette().to_vec();
        assert_eq!(
            moved,
            vec![
                Rgba::new(0, 20, 0, 255),
                Rgba::new(0, 0, 30, 128),
                Rgba::new(40, 50, 60, 255),
                Rgba::new(10, 0, 0, 255),
            ]
        );
        ed.undo();
        assert_eq!(ed.document().palette(), colors.as_slice());
        ed.redo();
        assert_eq!(ed.document().palette(), moved.as_slice());
        assert!(ed.move_palette_color(9, 0).is_err());
        assert_eq!(ed.document().palette(), moved.as_slice());

        for format in [
            crate::PaletteFormat::Gpl,
            crate::PaletteFormat::Hex,
            crate::PaletteFormat::Pal,
        ] {
            let text = crate::export_palette_string(&moved, format).unwrap();
            let back =
                crate::import_palette_bytes(text.as_bytes(), Some(format.extension())).unwrap();
            if format == crate::PaletteFormat::Pal {
                let opaque: Vec<_> = moved
                    .iter()
                    .map(|color| Rgba::new(color.r, color.g, color.b, 255))
                    .collect();
                assert_eq!(back, opaque);
            } else {
                assert_eq!(back, moved);
            }
        }
    }

    #[test]
    fn a_ramp_replaces_the_span_in_one_undo() {
        let mut ed = Editor::new(4, 4).unwrap();
        let colors = vec![
            Rgba::new(0, 0, 0, 255),
            Rgba::new(1, 1, 1, 255),
            Rgba::new(2, 2, 2, 255),
            Rgba::new(255, 0, 0, 255),
        ];
        ed.set_palette(colors.clone()).unwrap();
        ed.fill_palette_ramp(3, 0, 3).unwrap();
        assert_eq!(
            ed.document().palette(),
            &[
                Rgba::new(0, 0, 0, 255),
                Rgba::new(128, 0, 0, 255),
                Rgba::new(255, 0, 0, 255),
            ]
        );
        ed.undo();
        assert_eq!(ed.document().palette(), colors.as_slice());
        ed.redo();
        assert_eq!(ed.document().palette().len(), 3);

        let before = ed.document().palette().to_vec();
        let err = ed.fill_palette_ramp(0, 1, 256).unwrap_err();
        assert!(matches!(
            err,
            Error::Limit {
                what: "palette colors",
                ..
            }
        ));
        assert_eq!(ed.document().palette(), before.as_slice());
        assert!(ed.fill_palette_ramp(0, 0, 4).is_err());
        assert!(ed.fill_palette_ramp(0, 1, 1).is_err());
        assert_eq!(ed.document().palette(), before.as_slice());
    }

    #[test]
    fn snap_uses_the_nearest_swatch_for_pencil_fill_and_shapes() {
        let mut ed = Editor::new(4, 4).unwrap();
        let red = Rgba::new(255, 0, 0, 255);
        let green = Rgba::new(0, 255, 0, 255);
        ed.set_palette(vec![red, green]).unwrap();
        let chosen = Rgba::new(0, 200, 40, 255);
        ed.set_primary(chosen);
        ed.set_snap_to_palette(true);
        assert!(ed.snaps_to_palette());
        assert_eq!(ed.hover_color(false), green);

        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        assert_eq!(cel_pixel(&ed, 0, 0), green);
        assert_eq!(ed.primary(), chosen);
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 0, 0), Rgba::CLEAR);

        ed.set_tool(Tool::Bucket);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        assert_eq!(cel_pixel(&ed, 1, 1), green);
        assert_eq!(cel_pixel(&ed, 3, 3), green);

        ed.set_tool(Tool::Line);
        ed.set_primary(Rgba::new(240, 10, 10, 255));
        ed.pointer_down(0, 2, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        assert_eq!(cel_pixel(&ed, 0, 2), red);
        assert_eq!(cel_pixel(&ed, 2, 2), red);

        ed.set_tool(Tool::Eraser);
        ed.pointer_down(0, 2, MouseButton::Primary, false);
        ed.pointer_up(0, 2, false);
        assert_eq!(cel_pixel(&ed, 0, 2), Rgba::CLEAR);

        ed.set_snap_to_palette(false);
        ed.set_tool(Tool::Pencil);
        ed.set_primary(chosen);
        ed.pointer_down(0, 3, MouseButton::Primary, false);
        ed.pointer_up(0, 3, false);
        assert_eq!(cel_pixel(&ed, 0, 3), chosen);

        ed.set_tool(Tool::Eyedropper);
        ed.set_snap_to_palette(true);
        ed.pointer_down(0, 3, MouseButton::Primary, false);
        assert_eq!(ed.primary(), chosen);
    }

    /// Ink at (1,1), (2,1) and (1,2): an L whose bounding box is (1,1) to (2,2).
    fn paint_l(ed: &mut Editor, color: Rgba) {
        for (x, y) in [(1, 1), (2, 1), (1, 2)] {
            paint(ed, x, y, color);
        }
    }

    #[test]
    fn wand_selects_the_fill_area_and_shift_adds_alt_subtracts() {
        let mut ed = editor();
        paint_l(&mut ed, ink(1));
        paint(&mut ed, 5, 5, ink(1));
        ed.set_tool(Tool::Wand);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(1, 1, 2, 2)));
        assert!(ed.selection_is_masked());
        assert!(ed.is_selected(2, 1) && ed.is_selected(1, 2));
        assert!(!ed.is_selected(2, 2) && !ed.is_selected(5, 5));
        let length: i32 = ed
            .selection_outline()
            .iter()
            .map(|e| (e.x1 - e.x0) + (e.y1 - e.y0))
            .sum();
        assert_eq!(length, 8);

        ed.set_select_mode(SelectMode::Add);
        ed.pointer_down(5, 5, MouseButton::Primary, false);
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(1, 1, 5, 5)));
        assert!(ed.is_selected(5, 5) && ed.is_selected(1, 1));

        ed.set_select_mode(SelectMode::Subtract);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(5, 5, 5, 5)));
        assert!(!ed.selection_is_masked());

        // Tolerance reaches a near color; Global reaches every clear pixel.
        ed.set_select_mode(SelectMode::Replace);
        paint(&mut ed, 3, 1, ink(4));
        ed.set_fill_tolerance(3);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        assert!(ed.is_selected(3, 1) && !ed.is_selected(5, 5));
        ed.set_fill_tolerance(0);
        ed.set_fill_global(true);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        assert!(ed.is_selected(7, 7) && ed.is_selected(2, 2));
        assert!(!ed.is_selected(1, 1) && !ed.is_selected(5, 5));
        // A click off the cel keeps the selection, and selecting is not an undo step.
        ed.pointer_down(-1, 0, MouseButton::Primary, false);
        assert!(ed.is_selected(7, 7));
        assert!(!ed.can_undo());
    }

    #[test]
    fn scale_selection_resizes_from_its_corner_in_one_undo() {
        let mut ed = Editor::new(8, 8).unwrap();
        let ink = Rgba::new(200, 10, 10, 255);
        ed.set_primary(ink);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        ed.set_tool(Tool::Select);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        ed.scale_selection(4, 4).unwrap();
        let cel = ed.document().cel(0, 0).unwrap();
        let inked: Vec<usize> = (0..64).filter(|&i| cel[i] == ink).collect();
        assert_eq!(inked, [9, 10, 17, 18], "the one pixel is now 2x2 at (1,1)");
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(1, 1, 4, 4)));
        assert!(ed.undo());
        assert_eq!(
            ed.document()
                .cel(0, 0)
                .unwrap()
                .iter()
                .filter(|p| **p == ink)
                .count(),
            1
        );
        // Growing past the edge keeps only what fits.
        ed.scale_selection(16, 16).unwrap();
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(1, 1, 7, 7)));
        assert!(ed.scale_selection(0, 3).is_err());
    }

    #[test]
    fn outline_and_shadow_are_one_undo_step_each_and_respect_the_selection() {
        let mut ed = Editor::new(5, 5).unwrap();
        let ink = Rgba::new(255, 255, 255, 255);
        let line = Rgba::new(0, 0, 0, 255);
        ed.set_primary(ink);
        ed.pointer_down(2, 2, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        assert!(ed.outline(line, false).unwrap());
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel.iter().filter(|p| **p == line).count(), 4);
        assert!(ed.undo());
        assert!(ed.drop_shadow(line, 1, 1).unwrap());
        assert_eq!(ed.document().cel(0, 0).unwrap()[3 * 5 + 3], line);
        assert!(ed.undo());
        ed.set_tool(Tool::Select);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(4, 1, false);
        assert!(ed.outline(line, true).unwrap());
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(
            cel.iter().filter(|p| **p == line).count(),
            3,
            "only row 1 is selected"
        );
        assert!(!ed.drop_shadow(line, 0, 0).unwrap());
    }

    #[test]
    fn replace_color_covers_the_chosen_layers_and_frames_in_one_undo() {
        let mut ed = Editor::new(4, 4).unwrap();
        let old = Rgba::new(10, 20, 30, 255);
        let new = Rgba::new(200, 100, 50, 255);
        paint(&mut ed, 0, 0, old);
        paint(&mut ed, 3, 3, old);
        ed.add_layer().unwrap();
        paint(&mut ed, 1, 1, old);
        ed.add_frame().unwrap();
        paint(&mut ed, 2, 2, old);
        ed.set_active_frame(0).unwrap();
        let count = |ed: &Editor, frame, layer, color| {
            ed.document()
                .cel(frame, layer)
                .unwrap()
                .iter()
                .filter(|p| **p == color)
                .count()
        };
        // The active layer of the active frame only.
        assert_eq!(ed.replace_color(old, new, false, false).unwrap(), 1);
        assert_eq!(count(&ed, 0, 0, old), 2);
        assert!(ed.undo());
        // Every layer and frame is one undo step.
        let before = ed.document().clone();
        assert_eq!(ed.replace_color(old, new, true, true).unwrap(), 4);
        assert_eq!(count(&ed, 1, 1, new), 1,);
        assert!(ed.undo());
        assert_eq!(ed.document(), &before);
        // Inside the selection only, and nothing to change is no undo step.
        ed.set_active_layer(0).unwrap();
        ed.set_tool(Tool::Select);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        assert_eq!(ed.replace_color(old, new, false, false).unwrap(), 1);
        assert_eq!(ed.document().cel(0, 0).unwrap()[15], old);
        assert!(ed.undo());
        let generation = ed.generation();
        assert_eq!(ed.replace_color(new, old, true, true).unwrap(), 0);
        assert_eq!(ed.generation(), generation);
    }

    #[test]
    fn dither_alternates_both_colors_on_a_fixed_checkerboard() {
        let mut ed = Editor::new(4, 2).unwrap();
        let a = Rgba::new(255, 0, 0, 255);
        let b = Rgba::new(0, 0, 255, 255);
        ed.set_primary(a);
        ed.set_secondary(b);
        ed.set_dither(true);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_move(3, 0, false);
        ed.pointer_up(3, 0, false);
        let cel = ed.document().cel(0, 0).unwrap().to_vec();
        assert_eq!(&cel[..4], &[a, b, a, b]);
        // The right button swaps which color lands on the even squares.
        ed.pointer_down(0, 1, MouseButton::Secondary, false);
        ed.pointer_move(3, 1, false);
        ed.pointer_up(3, 1, false);
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(&cel[4..], &[a, b, a, b], "row 1 starts on an odd square");
        // Shapes and the eraser ignore it.
        ed.set_tool(Tool::Line);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(3, 0, false);
        assert_eq!(&ed.document().cel(0, 0).unwrap()[..4], &[a; 4]);
    }

    #[test]
    fn a_custom_brush_stamps_the_selected_art_in_its_own_colors() {
        let mut ed = Editor::new(8, 8).unwrap();
        let red = Rgba::new(255, 0, 0, 255);
        let blue = Rgba::new(0, 0, 255, 255);
        assert!(matches!(ed.brush_from_selection(), Err(Error::NoSelection)));
        paint(&mut ed, 0, 0, red);
        paint(&mut ed, 1, 0, blue);
        ed.set_tool(Tool::Select);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(2, 1, false);
        assert!(ed.brush_from_selection().unwrap());
        let brush = ed.custom_brush().unwrap();
        assert_eq!((brush.width, brush.height), (3, 2));
        ed.deselect();
        ed.set_tool(Tool::Pencil);
        // A 3×2 brush is centered with its top left at (x - 1, y - 1).
        ed.pointer_down(5, 5, MouseButton::Primary, false);
        ed.pointer_up(5, 5, false);
        let cel = ed.document().cel(0, 0).unwrap().to_vec();
        assert_eq!(cel[4 * 8 + 4], red);
        assert_eq!(cel[4 * 8 + 5], blue);
        assert_eq!(
            cel[4 * 8 + 6],
            Rgba::CLEAR,
            "clear brush pixels leave the canvas alone"
        );
        assert_eq!(cel.iter().filter(|p| p.a > 0).count(), 4);
        // The eraser clears the brush's shape only.
        ed.set_tool(Tool::Eraser);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        let cel = ed.document().cel(0, 0).unwrap();
        assert_eq!(cel[0], Rgba::CLEAR);
        assert_eq!(cel[1], Rgba::CLEAR);
        ed.clear_custom_brush();
        assert!(ed.custom_brush().is_none());
        // An empty selection makes no brush, and a huge one is refused.
        ed.set_tool(Tool::Select);
        ed.pointer_down(6, 0, MouseButton::Primary, false);
        ed.pointer_up(7, 1, false);
        assert!(!ed.brush_from_selection().unwrap());
        let mut big = Editor::new(MAX_CUSTOM_BRUSH + 1, 2).unwrap();
        big.select_all();
        assert!(matches!(
            big.brush_from_selection(),
            Err(Error::Limit { .. })
        ));
    }

    #[test]
    fn an_external_edit_is_one_undo_step_and_keeps_the_view() {
        let mut ed = Editor::new(4, 4).unwrap();
        ed.add_frame().unwrap();
        ed.set_active_frame(1).unwrap();
        let mut edited = ed.document().clone();
        edited.set_active_frame(0).unwrap();
        edited
            .set_pixel(0, 0, 1, 1, Rgba::new(9, 9, 9, 255))
            .unwrap();
        ed.apply_external(edited);
        assert!(ed.is_dirty());
        assert_eq!(ed.document().active_frame(), 1, "the view stays on frame 1");
        assert_eq!(ed.document().cel(0, 0).unwrap()[5], Rgba::new(9, 9, 9, 255));
        assert!(ed.undo());
        assert_eq!(ed.document().cel(0, 0).unwrap()[5], Rgba::CLEAR);
    }

    /// A 4×4 canvas with a tilemap layer of 2×2 tiles on top, active.
    fn tile_editor() -> Editor {
        let mut ed = Editor::new(4, 4).unwrap();
        ed.add_tilemap_layer(None, 2, 2).unwrap();
        ed
    }

    #[test]
    fn drawing_on_a_tilemap_edits_every_copy_of_the_tile() {
        let mut ed = tile_editor();
        let layer = ed.document().active_layer();
        let ink = Rgba::new(200, 0, 0, 255);
        ed.set_primary(ink);
        // A pixel on an empty cell makes tile 1 there.
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        assert_eq!(ed.document().tile_grid(0, layer).unwrap(), &[1, 0, 0, 0]);
        assert_eq!(ed.document().tilesets()[0].len(), 1);
        // Place it in the right cell, mirrored, in tile mode.
        ed.set_tile_mode(true);
        ed.set_selected_tile(1 | crate::tilemap::TILE_FLIP_X);
        ed.pointer_down(3, 0, MouseButton::Primary, false);
        ed.pointer_up(3, 0, false);
        let cel = ed.document().cel(0, layer).unwrap();
        assert_eq!(
            cel[3], ink,
            "the mirrored copy shows the pixel at its right edge"
        );
        // Back in pixel mode, a pixel on the left copy shows in the mirrored one.
        ed.set_tile_mode(false);
        let blue = Rgba::new(0, 0, 200, 255);
        ed.set_primary(blue);
        ed.pointer_down(0, 1, MouseButton::Primary, false);
        ed.pointer_up(0, 1, false);
        let cel = ed.document().cel(0, layer).unwrap();
        assert_eq!(cel[4], blue);
        assert_eq!(cel[7], blue, "the mirrored copy changed too");
        assert_eq!(ed.document().tilesets()[0].len(), 1, "no new tile");
        assert!(ed.undo());
        assert_eq!(ed.document().cel(0, layer).unwrap()[7], Rgba::CLEAR);
        assert_eq!(ed.document().tilesets()[0].tile(1).unwrap()[2], Rgba::CLEAR);
    }

    #[test]
    fn tile_mode_places_erases_fills_and_picks_whole_tiles() {
        let mut ed = tile_editor();
        let layer = ed.document().active_layer();
        ed.set_primary(Rgba::new(9, 9, 9, 255));
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(1, 1, false);
        ed.set_tile_mode(true);
        assert!(ed.placing_tiles());
        ed.set_tool(Tool::Bucket);
        ed.pointer_down(3, 3, MouseButton::Primary, false);
        ed.pointer_up(3, 3, false);
        assert_eq!(ed.document().tile_grid(0, layer).unwrap(), &[1, 1, 1, 1]);
        ed.set_tool(Tool::Eraser);
        ed.pointer_down(2, 0, MouseButton::Primary, false);
        ed.pointer_move(2, 3, false);
        ed.pointer_up(2, 3, false);
        assert_eq!(ed.document().tile_grid(0, layer).unwrap(), &[1, 0, 1, 0]);
        assert!(ed.undo(), "the erase stroke is one step");
        assert_eq!(ed.document().tile_grid(0, layer).unwrap(), &[1, 1, 1, 1]);
        ed.set_selected_tile(0);
        ed.set_tool(Tool::Eyedropper);
        ed.pointer_down(0, 3, MouseButton::Primary, false);
        assert_eq!(ed.selected_tile(), 1);
        // A new tile is a copy of the selected one.
        assert_eq!(ed.duplicate_tile().unwrap(), 2);
        assert_eq!(ed.selected_tile(), 2);
        // Tile mode does nothing on an ordinary layer.
        ed.set_active_layer(0).unwrap();
        assert!(!ed.placing_tiles());
    }

    #[test]
    fn frames_resizes_and_merges_keep_tilemaps_whole() {
        let mut ed = tile_editor();
        let layer = ed.document().active_layer();
        ed.set_primary(Rgba::new(5, 5, 5, 255));
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        ed.add_frame().unwrap();
        assert_eq!(ed.document().tile_grid(1, layer).unwrap(), &[0, 0, 0, 0]);
        ed.duplicate_frame().unwrap();
        ed.set_active_frame(0).unwrap();
        ed.duplicate_frame().unwrap();
        assert_eq!(ed.document().tile_grid(1, layer).unwrap(), &[1, 0, 0, 0]);
        // Growing the canvas by 2 on the left moves the cells one tile right.
        ed.resize_canvas(6, 4, 2, 0).unwrap();
        assert_eq!(
            ed.document().tile_grid(0, layer).unwrap(),
            &[0, 1, 0, 0, 0, 0]
        );
        assert_eq!(
            ed.document().cel(0, layer).unwrap()[2],
            Rgba::new(5, 5, 5, 255)
        );
        assert!(
            matches!(ed.document().clone().merge_down(layer), Ok(())),
            "a tilemap merges into the pixel layer below"
        );
        let mut doc = ed.document().clone();
        doc.add_layer("top").unwrap();
        assert!(matches!(doc.merge_down(layer + 1), Err(Error::Tilemap(_))));
    }

    #[test]
    fn shift_and_alt_marquees_add_and_cut_out() {
        let mut ed = editor();
        ed.set_tool(Tool::Select);
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(3, 3, false);
        ed.set_select_mode(SelectMode::Subtract);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_move(2, 2, false);
        assert_eq!(
            ed.marquee_drag(),
            Some((PixelRect::from_corners(1, 1, 2, 2), SelectMode::Subtract))
        );
        ed.pointer_up(2, 2, false);
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(0, 0, 3, 3)));
        assert!(ed.is_selected(0, 0) && !ed.is_selected(1, 2));
        // Pressing inside an added area starts a new marquee instead of a move.
        ed.set_select_mode(SelectMode::Add);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(0, 0, 3, 3)));
        assert!(!ed.selection_is_masked());
    }

    #[test]
    fn a_masked_selection_moves_clears_and_clips_only_its_pixels() {
        let mut ed = editor();
        paint_l(&mut ed, ink(1));
        paint(&mut ed, 2, 2, ink(2));
        ed.set_tool(Tool::Wand);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.set_tool(Tool::Select);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        ed.pointer_move(4, 1, false);
        ed.pointer_up(4, 1, false);
        for (x, y) in [(4, 1), (5, 1), (4, 2)] {
            assert_eq!(cel_pixel(&ed, x, y), ink(1));
        }
        assert_eq!(cel_pixel(&ed, 5, 2), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 1, 1), Rgba::CLEAR);
        // The unselected corner of the bounding box stays.
        assert_eq!(cel_pixel(&ed, 2, 2), ink(2));
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(4, 1, 5, 2)));
        assert!(!ed.is_selected(5, 2));
        // The shifted outline matches one traced at the new place.
        let traced = selection::outline(
            PixelRect::from_corners(4, 1, 5, 2),
            ed.selection_mask.as_deref(),
        );
        assert_eq!(ed.selection_outline(), traced.as_slice());

        // Undo puts the mask back with the pixels.
        assert!(ed.undo());
        assert_eq!(ed.selection(), Some(PixelRect::from_corners(1, 1, 2, 2)));
        assert!(ed.selection_is_masked() && !ed.is_selected(2, 2));

        ed.set_tool(Tool::Pencil);
        ed.set_primary(ink(9));
        ed.pointer_down(2, 2, MouseButton::Primary, false);
        ed.pointer_up(2, 2, false);
        assert_eq!(cel_pixel(&ed, 2, 2), ink(2));
        ed.pointer_down(2, 1, MouseButton::Primary, false);
        ed.pointer_up(2, 1, false);
        assert_eq!(cel_pixel(&ed, 2, 1), ink(9));

        ed.clear_selection_pixels().unwrap();
        assert_eq!(cel_pixel(&ed, 1, 1), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 2, 1), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 2, 2), ink(2));

        // The bucket stays inside the mask too.
        ed.set_tool(Tool::Bucket);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        assert_eq!(cel_pixel(&ed, 1, 2), ink(9));
        assert_eq!(cel_pixel(&ed, 0, 0), Rgba::CLEAR);
    }

    #[test]
    fn a_masked_copy_pastes_and_flips_its_shape() {
        let mut ed = editor();
        paint_l(&mut ed, ink(1));
        paint(&mut ed, 6, 6, ink(3));
        ed.set_tool(Tool::Wand);
        ed.pointer_down(1, 1, MouseButton::Primary, false);
        assert!(ed.copy_selection());
        ed.set_tool(Tool::Select);
        ed.pointer_down(5, 5, MouseButton::Primary, false);
        ed.pointer_up(5, 5, false);
        ed.paste_clipboard().unwrap();
        for (x, y) in [(5, 5), (6, 5), (5, 6)] {
            assert_eq!(cel_pixel(&ed, x, y), ink(1));
        }
        assert_eq!(cel_pixel(&ed, 6, 6), ink(3));
        assert!(ed.selection_is_masked() && !ed.is_selected(6, 6));

        // X X        X X
        // X .  flips . X, over the unselected pixel at (6, 6).
        ed.transform_pixels(Transform::FlipHorizontal).unwrap();
        assert_eq!(cel_pixel(&ed, 5, 6), Rgba::CLEAR);
        assert_eq!(cel_pixel(&ed, 6, 6), ink(1));
        assert!(ed.is_selected(6, 6) && !ed.is_selected(5, 6));
        assert!(ed.undo());
        assert_eq!(cel_pixel(&ed, 6, 6), ink(3));
        assert!(ed.is_selected(5, 6));
    }

    #[test]
    fn tags_are_undoable_and_follow_frame_edits() {
        use crate::tags::{FrameTag, LoopDirection};
        let mut ed = editor();
        ed.add_frame().unwrap();
        ed.add_frame().unwrap();
        let walk = FrameTag {
            name: "walk".into(),
            from: 1,
            to: 2,
            direction: LoopDirection::Forward,
            repeat: 0,
        };
        assert_eq!(ed.add_tag(walk.clone()).unwrap(), 0);
        // Setting the same tag records nothing.
        let before = ed.can_redo();
        ed.set_tag(0, walk.clone()).unwrap();
        assert_eq!(ed.can_redo(), before);
        let faster = FrameTag {
            direction: LoopDirection::PingPong,
            ..walk.clone()
        };
        ed.set_tag(0, faster.clone()).unwrap();
        assert!(ed
            .set_tag(
                0,
                FrameTag {
                    to: 9,
                    ..walk.clone()
                }
            )
            .is_err());
        // Copying frame 1 puts the copy at 2, inside the tag, which grows.
        ed.set_active_frame(1).unwrap();
        ed.duplicate_frame().unwrap();
        assert_eq!(
            (ed.document().tags()[0].from, ed.document().tags()[0].to),
            (1, 3)
        );
        assert!(ed.undo());
        assert_eq!(ed.document().tags()[0], faster);
        assert!(ed.undo());
        assert_eq!(ed.document().tags()[0], walk);
        // Deleting frame 0 moves the tag back by one.
        ed.set_active_frame(0).unwrap();
        ed.delete_frame().unwrap();
        assert_eq!(
            (ed.document().tags()[0].from, ed.document().tags()[0].to),
            (0, 1)
        );
        ed.remove_tag(0).unwrap();
        assert!(ed.document().tags().is_empty());
        assert!(ed.undo());
        assert_eq!(ed.document().tags().len(), 1);
    }

    #[test]
    fn renaming_a_frame_is_one_undo_and_copies_keep_the_name() {
        let mut ed = editor();
        ed.rename_frame("idle");
        assert_eq!(ed.document().frame_name(0), Some("idle"));
        let before = ed.can_undo();
        ed.rename_frame("idle");
        assert_eq!(ed.can_undo(), before);
        ed.duplicate_frame().unwrap();
        assert_eq!(ed.document().frame_name(1), Some("idle"));
        assert!(ed.undo());
        assert!(ed.undo());
        assert_eq!(ed.document().frame_name(0), None);
    }
}
