//! Pixel document: a shared layer stack and one cel per layer per frame.
//!
//! Layers are properties (name, visibility, opacity) shared by every frame.
//! Each frame stores its own pixels for those layers, plus a duration.

use crate::blend::{blend, BlendMode};
use crate::color::{source_over, Rgba};
use crate::error::{Error, Result};
use crate::limits::{
    check_stack, clamp_duration, clamp_name, DEFAULT_FRAME_MS, MAX_GROUP_DEPTH, MAX_PALETTE,
    MAX_TAGS,
};
use crate::tags::{self, FrameTag};
use crate::tilemap::{Cel, Tileset};

/// Name, visibility, opacity, and blend mode shared by every frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerInfo {
    pub name: String,
    pub visible: bool,
    /// 0 is hidden, 255 is fully opaque. Applied at composite time.
    pub opacity: u8,
    /// How the layer mixes with what is below it. A document with any mode
    /// other than Normal is saved with a required `.oxypix` section.
    pub blend: BlendMode,
    /// A group holds the layers right after it that are one level deeper.
    /// Its own cels stay clear and are never drawn.
    pub group: bool,
    /// How many groups this layer is inside. 0 is the top level.
    pub depth: u8,
    /// On a tilemap layer, the tileset its grid refers to.
    pub tileset: Option<u16>,
}

impl LayerInfo {
    /// A visible, opaque, Normal layer at the top level.
    pub fn new(name: String) -> Self {
        Self {
            name,
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
            group: false,
            depth: 0,
            tileset: None,
        }
    }
}

/// Whether depths and group flags describe a tree: the first layer is at the
/// top level, and a layer one level deeper than the one before it follows a
/// group.
pub(crate) fn valid_layer_tree(layers: &[LayerInfo]) -> bool {
    let mut previous: Option<&LayerInfo> = None;
    for layer in layers {
        if layer.depth > MAX_GROUP_DEPTH {
            return false;
        }
        let ok = match previous {
            None => layer.depth == 0,
            Some(prev) => {
                layer.depth <= prev.depth || (layer.depth == prev.depth + 1 && prev.group)
            }
        };
        if !ok {
            return false;
        }
        previous = Some(layer);
    }
    layers.iter().any(|layer| !layer.group)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub duration_ms: u32,
    /// Empty when the frame has no name. Saved in `.oxypix` version 2.
    pub name: String,
    /// One cel per layer, row-major pixels of length `width * height`.
    pub cels: Vec<Cel>,
}

/// A pixel-art document. Fields stay private so the layer/frame invariant holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    width: u32,
    height: u32,
    layers: Vec<LayerInfo>,
    frames: Vec<Frame>,
    active_frame: usize,
    active_layer: usize,
    palette: Vec<Rgba>,
    /// Animation tags. A document with any is saved as `.oxypix` version 2.
    tags: Vec<FrameTag>,
    /// The tilesets tilemap layers draw from.
    tilesets: Vec<Tileset>,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        check_stack(width, height, 1, 1)?;
        Ok(Self {
            width,
            height,
            layers: vec![LayerInfo::new("Layer 1".to_string())],
            frames: vec![Frame {
                duration_ms: DEFAULT_FRAME_MS,
                name: String::new(),
                cels: vec![vec![Rgba::CLEAR; (width * height) as usize].into()],
            }],
            active_frame: 0,
            active_layer: 0,
            palette: default_palette(),
            tags: Vec::new(),
            tilesets: Vec::new(),
        })
    }

    /// A two-frame sample used by File → Open example and by tests.
    pub fn demo() -> Result<Self> {
        let mut doc = Self::new(32, 32)?;
        doc.layers[0].name = "Paper".to_string();
        for px in doc.frames[0].cels[0].iter_mut() {
            *px = Rgba::new(20, 22, 30, 255);
        }
        doc.add_layer("Mark")?;
        let amber = Rgba::new(232, 168, 56, 255);
        let teal = Rgba::new(58, 161, 122, 255);
        for (x, y, color) in mark(12, 12, amber, teal) {
            doc.set_pixel(0, 1, x, y, color)?;
        }
        doc.duplicate_frame(0)?;
        doc.frames[1].cels[1].fill(Rgba::CLEAR);
        for (x, y, color) in mark(14, 13, amber, teal) {
            doc.set_pixel(1, 1, x, y, color)?;
        }
        doc.frames[0].duration_ms = 180;
        doc.frames[1].duration_ms = 180;
        doc.active_frame = 0;
        doc.active_layer = 1;
        Ok(doc)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn active_frame(&self) -> usize {
        self.active_frame
    }

    pub fn active_layer(&self) -> usize {
        self.active_layer
    }

    pub fn layer(&self, index: usize) -> Option<&LayerInfo> {
        self.layers.get(index)
    }

    pub fn layers(&self) -> &[LayerInfo] {
        &self.layers
    }

    pub fn frame_duration(&self, index: usize) -> Option<u32> {
        self.frames.get(index).map(|f| f.duration_ms)
    }

    /// The frame's name, or `None` when it has none. A name of only spaces
    /// counts as none.
    pub fn frame_name(&self, index: usize) -> Option<&str> {
        self.frames
            .get(index)
            .map(|f| f.name.as_str())
            .filter(|name| !name.trim().is_empty())
    }

    /// Name a frame, cut to the name limit. It is kept as typed, so a name
    /// can be edited one key at a time; an empty name removes it.
    pub fn set_frame_name(&mut self, index: usize, name: &str) -> Result<()> {
        let frame = self
            .frames
            .get_mut(index)
            .ok_or(Error::Index { what: "frame" })?;
        frame.name = clamp_name(name);
        Ok(())
    }

    pub fn palette(&self) -> &[Rgba] {
        &self.palette
    }

    /// Animation tags, in the order they were added.
    pub fn tags(&self) -> &[FrameTag] {
        &self.tags
    }

    /// Add a tag. Returns its index. The name is trimmed and cut to the name
    /// limit; a blank name or a range outside the frames is an error.
    pub fn add_tag(&mut self, tag: FrameTag) -> Result<usize> {
        if self.tags.len() >= MAX_TAGS {
            return Err(Error::Limit {
                what: "tags",
                value: self.tags.len() as u64 + 1,
                max: MAX_TAGS as u64,
            });
        }
        self.tags.push(tags::checked(tag, self.frames.len())?);
        Ok(self.tags.len() - 1)
    }

    /// Replace the tag at `index`, with the same checks as [`Self::add_tag`].
    pub fn set_tag(&mut self, index: usize, tag: FrameTag) -> Result<()> {
        let tag = tags::checked(tag, self.frames.len())?;
        let slot = self
            .tags
            .get_mut(index)
            .ok_or(Error::Index { what: "tag" })?;
        *slot = tag;
        Ok(())
    }

    pub fn remove_tag(&mut self, index: usize) -> Result<()> {
        if index >= self.tags.len() {
            return Err(Error::Index { what: "tag" });
        }
        self.tags.remove(index);
        Ok(())
    }

    /// Replace every tag at once, as the file reader does. All are checked first.
    pub(crate) fn set_tags(&mut self, list: Vec<FrameTag>) -> Result<()> {
        if list.len() > MAX_TAGS {
            return Err(Error::Limit {
                what: "tags",
                value: list.len() as u64,
                max: MAX_TAGS as u64,
            });
        }
        let checked = list
            .into_iter()
            .map(|tag| tags::checked(tag, self.frames.len()))
            .collect::<Result<Vec<_>>>()?;
        self.tags = checked;
        Ok(())
    }

    pub fn set_active_frame(&mut self, index: usize) -> Result<()> {
        if index >= self.frames.len() {
            return Err(Error::Index { what: "frame" });
        }
        self.active_frame = index;
        Ok(())
    }

    pub fn set_active_layer(&mut self, index: usize) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        self.active_layer = index;
        Ok(())
    }

    pub fn pixel_bytes(&self) -> usize {
        let cels =
            self.frames.len() * self.layers.len() * (self.width as usize) * (self.height as usize);
        let tiles: usize = self
            .tilesets
            .iter()
            .map(|set| set.len() * set.tile_pixels())
            .sum();
        let grids: usize = self
            .frames
            .iter()
            .flat_map(|frame| &frame.cels)
            .filter_map(|cel| cel.tiles.as_ref().map(Vec::len))
            .sum();
        (cels + tiles + grids) * 4
    }

    #[cfg(test)]
    pub(crate) fn frames_mut(&mut self) -> &mut [Frame] {
        &mut self.frames
    }

    pub(crate) fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub(crate) fn layers_mut(&mut self) -> &mut [LayerInfo] {
        &mut self.layers
    }

    #[cfg(test)]
    pub(crate) fn palette_mut(&mut self) -> &mut Vec<Rgba> {
        &mut self.palette
    }

    pub fn cel(&self, frame: usize, layer: usize) -> Result<&[Rgba]> {
        self.frames
            .get(frame)
            .and_then(|f| f.cels.get(layer))
            .map(|cel| cel.pixels.as_slice())
            .ok_or(Error::Index { what: "cel" })
    }

    /// A layer's pixels to change. A group has none to draw on.
    pub(crate) fn cel_mut(&mut self, frame: usize, layer: usize) -> Result<&mut [Rgba]> {
        if self.layers.get(layer).is_some_and(|l| l.group) {
            return Err(Error::Group(
                "a group has no pixels; pick a layer inside it",
            ));
        }
        self.frames
            .get_mut(frame)
            .and_then(|f| f.cels.get_mut(layer))
            .map(|cel| cel.pixels.as_mut_slice())
            .ok_or(Error::Index { what: "cel" })
    }

    pub(crate) fn set_pixel(
        &mut self,
        frame: usize,
        layer: usize,
        x: i32,
        y: i32,
        color: Rgba,
    ) -> Result<()> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return Err(Error::Index { what: "pixel" });
        }
        let w = self.width;
        let cel = self.cel_mut(frame, layer)?;
        cel[(y as u32 * w + x as u32) as usize] = color;
        Ok(())
    }

    pub fn add_layer(&mut self, name: &str) -> Result<()> {
        check_stack(
            self.width,
            self.height,
            self.frames.len(),
            self.layers.len() + 1,
        )?;
        let name = if name.is_empty() {
            format!("Layer {}", self.layers.len() + 1)
        } else {
            clamp_name(name)
        };
        self.layers.push(LayerInfo::new(name));
        let len = (self.width * self.height) as usize;
        for frame in &mut self.frames {
            frame.cels.push(vec![Rgba::CLEAR; len].into());
        }
        self.active_layer = self.layers.len() - 1;
        Ok(())
    }

    /// Add an empty group on top of the stack.
    pub fn add_group(&mut self, name: &str) -> Result<()> {
        let groups = self.layers.iter().filter(|l| l.group).count();
        let name = if name.is_empty() {
            format!("Group {}", groups + 1)
        } else {
            name.to_string()
        };
        self.add_layer(&name)?;
        if let Some(layer) = self.layers.last_mut() {
            layer.group = true;
        }
        Ok(())
    }

    /// Put a new group where `index` is and move `index` (with everything
    /// inside it) into that group.
    pub fn wrap_in_group(&mut self, index: usize, name: &str) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        let end = self.layer_block_end(index);
        if self.layers[index..end]
            .iter()
            .any(|l| l.depth >= MAX_GROUP_DEPTH)
        {
            return Err(Error::Group("groups cannot nest any deeper"));
        }
        check_stack(
            self.width,
            self.height,
            self.frames.len(),
            self.layers.len() + 1,
        )?;
        let groups = self.layers.iter().filter(|l| l.group).count();
        let name = if name.is_empty() {
            format!("Group {}", groups + 1)
        } else {
            clamp_name(name)
        };
        let group = LayerInfo {
            group: true,
            depth: self.layers[index].depth,
            ..LayerInfo::new(name)
        };
        for layer in &mut self.layers[index..end] {
            layer.depth += 1;
        }
        self.layers.insert(index, group);
        let len = (self.width * self.height) as usize;
        for frame in &mut self.frames {
            frame.cels.insert(index, vec![Rgba::CLEAR; len].into());
        }
        self.active_layer = index;
        Ok(())
    }

    /// One past the last layer in `index`'s block: the layer itself and, for
    /// a group, everything inside it.
    pub fn layer_block_end(&self, index: usize) -> usize {
        let Some(depth) = self.layers.get(index).map(|l| l.depth) else {
            return index;
        };
        let mut end = index + 1;
        while end < self.layers.len() && self.layers[end].depth > depth {
            end += 1;
        }
        end
    }

    /// The group `index` is directly inside.
    pub fn layer_parent(&self, index: usize) -> Option<usize> {
        let depth = self.layers.get(index)?.depth;
        if depth == 0 {
            return None;
        }
        (0..index).rev().find(|&k| self.layers[k].depth < depth)
    }

    /// The layer or group just below `index` in the same group.
    fn prev_sibling(&self, index: usize) -> Option<usize> {
        let depth = self.layers.get(index)?.depth;
        for k in (0..index).rev() {
            match self.layers[k].depth {
                d if d < depth => return None,
                d if d == depth => return Some(k),
                _ => {}
            }
        }
        None
    }

    /// The layer or group just above `index`'s block in the same group.
    fn next_sibling(&self, index: usize) -> Option<usize> {
        let depth = self.layers.get(index)?.depth;
        let end = self.layer_block_end(index);
        (end < self.layers.len() && self.layers[end].depth == depth).then_some(end)
    }

    /// Whether the layer and every group around it are visible.
    pub fn layer_shown(&self, index: usize) -> bool {
        let mut at = Some(index);
        while let Some(i) = at {
            if !self.layers.get(i).is_some_and(|l| l.visible) {
                return false;
            }
            at = self.layer_parent(i);
        }
        true
    }

    /// Layer indices in the order a layer panel lists them, top first, with
    /// each group above the layers inside it.
    pub fn layer_display_order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.layers.len());
        self.list_siblings(0, self.layers.len(), &mut order);
        order
    }

    fn list_siblings(&self, start: usize, end: usize, order: &mut Vec<usize>) {
        let mut heads = Vec::new();
        let mut i = start;
        while i < end {
            heads.push(i);
            i = self.layer_block_end(i);
        }
        for head in heads.into_iter().rev() {
            order.push(head);
            if self.layers[head].group {
                self.list_siblings(head + 1, self.layer_block_end(head), order);
            }
        }
    }

    /// Rearrange layers (and every frame's cels): `order[new] = old`.
    fn reorder(&mut self, order: &[usize]) {
        self.layers = order.iter().map(|&old| self.layers[old].clone()).collect();
        for frame in &mut self.frames {
            let mut old: Vec<Option<Cel>> = std::mem::take(&mut frame.cels)
                .into_iter()
                .map(Some)
                .collect();
            frame.cels = order
                .iter()
                .map(|&o| old[o].take().unwrap_or_default())
                .collect();
        }
        if let Some(active) = order.iter().position(|&o| o == self.active_layer) {
            self.active_layer = active;
        }
    }

    /// Move a layer (or a whole group) into the group just below it, as that
    /// group's top layer.
    pub fn indent_layer(&mut self, index: usize) -> Result<()> {
        self.prev_sibling(index)
            .filter(|&k| self.layers[k].group)
            .ok_or(Error::Group("there is no group just below this layer"))?;
        let end = self.layer_block_end(index);
        if self.layers[index..end]
            .iter()
            .any(|l| l.depth >= MAX_GROUP_DEPTH)
        {
            return Err(Error::Group("groups cannot nest any deeper"));
        }
        for layer in &mut self.layers[index..end] {
            layer.depth += 1;
        }
        Ok(())
    }

    /// Move a layer (or a whole group) out of its group, just above it.
    pub fn outdent_layer(&mut self, index: usize) -> Result<()> {
        let parent = self
            .layer_parent(index)
            .ok_or(Error::Group("this layer is not in a group"))?;
        let end = self.layer_block_end(index);
        let parent_end = self.layer_block_end(parent);
        let order: Vec<usize> = (0..index)
            .chain(end..parent_end)
            .chain(index..end)
            .chain(parent_end..self.layers.len())
            .collect();
        self.reorder(&order);
        let start = parent_end - (end - index);
        for layer in &mut self.layers[start..parent_end] {
            layer.depth -= 1;
        }
        Ok(())
    }

    pub fn duplicate_layer(&mut self, index: usize) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        check_stack(
            self.width,
            self.height,
            self.frames.len(),
            self.layers.len() + 1,
        )?;
        let end = self.layer_block_end(index);
        check_stack(
            self.width,
            self.height,
            self.frames.len(),
            self.layers.len() + (end - index),
        )?;
        let mut block: Vec<LayerInfo> = self.layers[index..end].to_vec();
        block[0].name = clamp_name(&format!("{} copy", block[0].name));
        self.layers.splice(end..end, block);
        for frame in &mut self.frames {
            let copy: Vec<Cel> = frame.cels[index..end].to_vec();
            frame.cels.splice(end..end, copy);
        }
        self.active_layer = end;
        Ok(())
    }

    pub fn delete_layer(&mut self, index: usize) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        let end = self.layer_block_end(index);
        let keeps_a_layer = self.layers[..index]
            .iter()
            .chain(&self.layers[end..])
            .any(|l| !l.group);
        if !keeps_a_layer {
            return Err(Error::LastLayer);
        }
        self.layers.drain(index..end);
        for frame in &mut self.frames {
            frame.cels.drain(index..end);
        }
        let removed = end - index;
        if self.active_layer >= end {
            self.active_layer -= removed;
        } else if self.active_layer >= index {
            self.active_layer = index.min(self.layers.len() - 1);
        }
        Ok(())
    }

    /// Move a layer (or a whole group) past `delta` neighbors in the same
    /// group. Positive is toward the top of the stack.
    pub fn move_layer(&mut self, index: usize, delta: i32) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        let mut at = index;
        for _ in 0..delta.unsigned_abs() {
            let (low, high) = if delta > 0 {
                (at, self.next_sibling(at))
            } else {
                match self.prev_sibling(at) {
                    Some(prev) => (prev, Some(at)),
                    None => (at, None),
                }
            };
            let high = high.ok_or(Error::Index { what: "layer" })?;
            let low_end = high;
            let high_end = self.layer_block_end(high);
            let order: Vec<usize> = (0..low)
                .chain(high..high_end)
                .chain(low..low_end)
                .chain(high_end..self.layers.len())
                .collect();
            self.reorder(&order);
            // The moving block was low..low_end going up, or high..high_end going down.
            at = if delta > 0 {
                low + (high_end - high)
            } else {
                low
            };
        }
        Ok(())
    }

    /// Bake `index` onto the layer below on every frame, then remove it.
    pub fn merge_down(&mut self, index: usize) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        let below = self.prev_sibling(index).ok_or(Error::BottomLayer)?;
        if self.layers[index].group || self.layers[below].group {
            return Err(Error::Group(
                "groups cannot be merged; merge the layers inside",
            ));
        }
        if self.layers[below].tileset.is_some() {
            return Err(Error::Tilemap(
                "a layer cannot be merged into a tilemap layer",
            ));
        }
        // An image layer's block is itself, so the sibling below is index - 1.
        debug_assert_eq!(below, index - 1);
        let upper_opacity = self.layers[index].opacity;
        let upper_blend = self.layers[index].blend;
        let lower_opacity = self.layers[index - 1].opacity;
        for frame in &mut self.frames {
            let upper = frame.cels[index].clone();
            let lower = &mut frame.cels[index - 1];
            for (dst, src) in lower.iter_mut().zip(upper.iter()) {
                let base = source_over(Rgba::CLEAR, *dst, lower_opacity);
                *dst = blend(base, *src, upper_opacity, upper_blend);
            }
        }
        self.layers[index - 1].opacity = 255;
        self.delete_layer(index)?;
        self.active_layer = index - 1;
        Ok(())
    }

    pub fn add_frame(&mut self) -> Result<usize> {
        check_stack(
            self.width,
            self.height,
            self.frames.len() + 1,
            self.layers.len(),
        )?;
        let cels = self.blank_cels();
        self.frames.push(Frame {
            duration_ms: DEFAULT_FRAME_MS,
            name: String::new(),
            cels,
        });
        // Appended after every tag, so no tag changes.
        self.active_frame = self.frames.len() - 1;
        Ok(self.active_frame)
    }

    pub fn duplicate_frame(&mut self, index: usize) -> Result<usize> {
        if index >= self.frames.len() {
            return Err(Error::Index { what: "frame" });
        }
        check_stack(
            self.width,
            self.height,
            self.frames.len() + 1,
            self.layers.len(),
        )?;
        let copy = self.frames[index].clone();
        self.frames.insert(index + 1, copy);
        tags::frame_inserted(&mut self.tags, index + 1);
        self.active_frame = index + 1;
        Ok(self.active_frame)
    }

    pub fn delete_frame(&mut self, index: usize) -> Result<()> {
        if self.frames.len() == 1 {
            return Err(Error::LastFrame);
        }
        if index >= self.frames.len() {
            return Err(Error::Index { what: "frame" });
        }
        self.frames.remove(index);
        tags::frame_removed(&mut self.tags, index);
        if self.active_frame >= self.frames.len() {
            self.active_frame = self.frames.len() - 1;
        } else if self.active_frame > index {
            self.active_frame -= 1;
        }
        Ok(())
    }

    pub fn set_frame_duration(&mut self, index: usize, ms: u32) -> Result<()> {
        let frame = self
            .frames
            .get_mut(index)
            .ok_or(Error::Index { what: "frame" })?;
        frame.duration_ms = clamp_duration(ms);
        Ok(())
    }

    pub fn add_palette_color(&mut self, color: Rgba) -> Result<()> {
        if self.palette.len() >= MAX_PALETTE {
            return Err(Error::PaletteFull(MAX_PALETTE));
        }
        self.palette.push(color);
        Ok(())
    }

    pub fn remove_palette_color(&mut self, index: usize) -> Result<()> {
        if self.palette.len() == 1 {
            return Err(Error::EmptyPalette);
        }
        if index >= self.palette.len() {
            return Err(Error::Index {
                what: "palette color",
            });
        }
        self.palette.remove(index);
        Ok(())
    }

    /// Move the swatch at `from` so it occupies `to` afterwards.
    pub fn move_palette_color(&mut self, from: usize, to: usize) -> Result<()> {
        let len = self.palette.len();
        if from >= len || to >= len {
            return Err(Error::Index {
                what: "palette color",
            });
        }
        if from == to {
            return Ok(());
        }
        let color = self.palette.remove(from);
        self.palette.insert(to, color);
        Ok(())
    }

    /// Replace the palette. An empty list, or one past the palette cap, is an error.
    pub fn set_palette(&mut self, colors: Vec<Rgba>) -> Result<()> {
        if colors.is_empty() {
            return Err(Error::EmptyPalette);
        }
        if colors.len() > MAX_PALETTE {
            return Err(Error::Limit {
                what: "palette colors",
                value: colors.len() as u64,
                max: MAX_PALETTE as u64,
            });
        }
        self.palette = colors;
        Ok(())
    }

    /// Change the canvas size. Existing pixels land at `(x + shift_x, y + shift_y)`.
    ///
    /// A positive shift adds transparent margin on the left or top. Pixels that
    /// fall outside the new canvas are dropped. The active frame and layer stay put.
    pub fn resize(&mut self, width: u32, height: u32, shift_x: i32, shift_y: i32) -> Result<()> {
        check_stack(width, height, self.frames.len(), self.layers.len())?;
        let new_len = (width as usize)
            .checked_mul(height as usize)
            .ok_or(Error::Limit {
                what: "pixels in one cel",
                value: u64::MAX,
                max: crate::limits::MAX_CEL_PIXELS,
            })?;
        for frame in &mut self.frames {
            for cel in &mut frame.cels {
                let mut next = vec![Rgba::CLEAR; new_len];
                for y in 0..self.height {
                    for x in 0..self.width {
                        let nx = i64::from(x) + i64::from(shift_x);
                        let ny = i64::from(y) + i64::from(shift_y);
                        if nx < 0 || ny < 0 || nx >= i64::from(width) || ny >= i64::from(height) {
                            continue;
                        }
                        let src = (y * self.width + x) as usize;
                        let dst = (ny as u32 * width + nx as u32) as usize;
                        next[dst] = cel[src];
                    }
                }
                cel.pixels = next;
            }
        }
        let old_width = self.width;
        let old_height = self.height;
        self.width = width;
        self.height = height;
        self.resize_grids(old_width, old_height, shift_x, shift_y);
        Ok(())
    }

    /// Composite one frame, bottom layer first. Hidden layers are skipped.
    pub fn composite(&self, frame: usize) -> Result<Vec<Rgba>> {
        let frame = self
            .frames
            .get(frame)
            .ok_or(Error::Index { what: "frame" })?;
        let mut out = vec![Rgba::CLEAR; (self.width * self.height) as usize];
        self.draw_layers(frame, 0, self.layers.len(), &mut out);
        Ok(out)
    }

    /// Draw the sibling layers in `start..end` (and what is inside their
    /// groups) onto `out`, bottom first. A group is drawn into its own
    /// buffer, then mixed in with the group's opacity and blend mode.
    fn draw_layers(&self, frame: &Frame, start: usize, end: usize, out: &mut [Rgba]) {
        let mut i = start;
        while i < end {
            let info = &self.layers[i];
            let block_end = self.layer_block_end(i);
            if info.visible && info.opacity > 0 {
                if info.group {
                    let mut inside = vec![Rgba::CLEAR; out.len()];
                    self.draw_layers(frame, i + 1, block_end, &mut inside);
                    for (dst, src) in out.iter_mut().zip(inside) {
                        *dst = blend(*dst, src, info.opacity, info.blend);
                    }
                } else if let Some(cel) = frame.cels.get(i) {
                    for (dst, src) in out.iter_mut().zip(cel.iter()) {
                        *dst = blend(*dst, *src, info.opacity, info.blend);
                    }
                }
            }
            i = block_end;
        }
    }

    fn layers_pixel(&self, frame: &Frame, start: usize, end: usize, index: usize) -> Rgba {
        let mut color = Rgba::CLEAR;
        let mut i = start;
        while i < end {
            let info = &self.layers[i];
            let block_end = self.layer_block_end(i);
            if info.visible && info.opacity > 0 {
                let src = if info.group {
                    self.layers_pixel(frame, i + 1, block_end, index)
                } else {
                    frame
                        .cels
                        .get(i)
                        .and_then(|cel| cel.get(index))
                        .copied()
                        .unwrap_or(Rgba::CLEAR)
                };
                color = blend(color, src, info.opacity, info.blend);
            }
            i = block_end;
        }
        color
    }

    /// The composited color of one pixel, using the same rules as [`Self::composite`].
    pub fn composite_pixel(&self, frame: usize, x: u32, y: u32) -> Option<Rgba> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let frame = self.frames.get(frame)?;
        let index = (y * self.width + x) as usize;
        Some(self.layers_pixel(frame, 0, self.layers.len(), index))
    }

    /// A document built from layers and frames, checked the way a loaded
    /// file is. Each frame is its duration in milliseconds and one cel per
    /// layer, `width * height` pixels each. A group's cel is cleared. Tags
    /// are checked against the frame count.
    pub fn from_layers(
        width: u32,
        height: u32,
        layers: Vec<LayerInfo>,
        frames: Vec<(u32, Vec<Vec<Rgba>>)>,
        palette: Vec<Rgba>,
        tags: Vec<FrameTag>,
    ) -> Result<Self> {
        let frames = frames
            .into_iter()
            .map(|(duration_ms, mut cels)| {
                for (cel, layer) in cels.iter_mut().zip(&layers) {
                    if layer.group {
                        cel.fill(Rgba::CLEAR);
                    }
                }
                Frame {
                    duration_ms,
                    name: String::new(),
                    cels: cels.into_iter().map(Cel::from).collect(),
                }
            })
            .collect();
        let mut doc = Self::from_parts(width, height, layers, frames, 0, 0, palette)?;
        if doc.layers.first().is_some_and(|layer| layer.group) {
            doc.active_layer = doc
                .layers
                .iter()
                .position(|layer| !layer.group)
                .unwrap_or(0);
        }
        for tag in tags {
            doc.add_tag(tag)?;
        }
        Ok(doc)
    }

    pub(crate) fn from_parts(
        width: u32,
        height: u32,
        layers: Vec<LayerInfo>,
        frames: Vec<Frame>,
        active_frame: usize,
        active_layer: usize,
        palette: Vec<Rgba>,
    ) -> Result<Self> {
        check_stack(width, height, frames.len(), layers.len())?;
        if palette.is_empty() {
            return Err(Error::EmptyPalette);
        }
        if palette.len() > MAX_PALETTE {
            return Err(Error::PaletteFull(MAX_PALETTE));
        }
        let expected = (width * height) as usize;
        for layer in &layers {
            if layer.name.len() > crate::limits::MAX_NAME_BYTES {
                return Err(Error::BadName);
            }
        }
        if !valid_layer_tree(&layers) {
            return Err(Error::BadFlag("layer groups"));
        }
        for frame in &frames {
            if frame.duration_ms < crate::limits::MIN_FRAME_MS
                || frame.duration_ms > crate::limits::MAX_FRAME_MS
            {
                return Err(Error::BadFlag("frame duration"));
            }
            if frame.cels.len() != layers.len() {
                return Err(Error::Truncated);
            }
            for cel in &frame.cels {
                if cel.len() != expected {
                    return Err(Error::Truncated);
                }
            }
        }
        if active_frame >= frames.len() || active_layer >= layers.len() {
            return Err(Error::Index { what: "active cel" });
        }
        Ok(Self {
            width,
            height,
            layers,
            frames,
            active_frame,
            active_layer,
            palette,
            tags: Vec::new(),
            tilesets: Vec::new(),
        })
    }
}

/// Tilemap layers.
impl Document {
    pub fn tilesets(&self) -> &[Tileset] {
        &self.tilesets
    }

    pub fn tileset(&self, index: usize) -> Option<&Tileset> {
        self.tilesets.get(index)
    }

    /// The tileset a layer's grid refers to, when it is a tilemap layer.
    pub fn layer_tileset(&self, layer: usize) -> Option<usize> {
        self.layers
            .get(layer)
            .and_then(|l| l.tileset)
            .map(usize::from)
    }

    /// Add an empty tileset and return its index.
    pub fn add_tileset(&mut self, name: &str, tile_width: u32, tile_height: u32) -> Result<usize> {
        if self.tilesets.len() >= crate::tilemap::MAX_TILESETS {
            return Err(Error::Limit {
                what: "tilesets",
                value: self.tilesets.len() as u64 + 1,
                max: crate::tilemap::MAX_TILESETS as u64,
            });
        }
        let name = if name.is_empty() {
            format!("Tileset {}", self.tilesets.len() + 1)
        } else {
            clamp_name(name)
        };
        self.tilesets
            .push(Tileset::new(name, tile_width, tile_height)?);
        Ok(self.tilesets.len() - 1)
    }

    /// Add a tilemap layer on top of the stack, with an empty grid in every frame.
    pub fn add_tilemap_layer(&mut self, name: &str, tileset: usize) -> Result<()> {
        if tileset >= self.tilesets.len() {
            return Err(Error::Index { what: "tileset" });
        }
        let maps = self.layers.iter().filter(|l| l.tileset.is_some()).count();
        let name = if name.is_empty() {
            format!("Tilemap {}", maps + 1)
        } else {
            name.to_string()
        };
        self.add_layer(&name)?;
        let cells = self.grid_cells(tileset);
        if let Some(layer) = self.layers.last_mut() {
            layer.tileset = Some(tileset as u16);
        }
        for frame in &mut self.frames {
            if let Some(cel) = frame.cels.last_mut() {
                cel.tiles = Some(vec![0; cells]);
            }
        }
        Ok(())
    }

    /// A tilemap layer's grid in one frame, row by row.
    pub fn tile_grid(&self, frame: usize, layer: usize) -> Option<&[u32]> {
        self.frames.get(frame)?.cels.get(layer)?.tiles.as_deref()
    }

    /// Put `value` (a tile number with flip bits, or 0 for empty) in a cell.
    /// `Ok(false)` when the cell already held it.
    pub fn set_tile(
        &mut self,
        frame: usize,
        layer: usize,
        col: u32,
        row: u32,
        value: u32,
    ) -> Result<bool> {
        let set = self
            .layer_tileset(layer)
            .ok_or(Error::Tilemap("this is not a tilemap layer"))?;
        let tileset = &self.tilesets[set];
        if !tileset.valid_entry(value) {
            return Err(Error::Index { what: "tile" });
        }
        let (cols, rows) = tileset.grid_size(self.width, self.height);
        if col >= cols || row >= rows {
            return Err(Error::Index { what: "tile cell" });
        }
        let (width, height) = (self.width, self.height);
        let cel = self
            .frames
            .get_mut(frame)
            .and_then(|f| f.cels.get_mut(layer))
            .ok_or(Error::Index { what: "cel" })?;
        let Some(grid) = cel.tiles.as_mut() else {
            return Err(Error::Tilemap("this is not a tilemap layer"));
        };
        let slot = &mut grid[(row * cols + col) as usize];
        if *slot == value {
            return Ok(false);
        }
        *slot = value;
        let (tw, th) = (tileset.tile_width(), tileset.tile_height());
        for y in row * th..((row + 1) * th).min(height) {
            for x in col * tw..((col + 1) * tw).min(width) {
                cel.pixels[(y * width + x) as usize] = tileset.pixel(value, x, y);
            }
        }
        Ok(true)
    }

    /// Add a tile to a tileset and return its number.
    pub fn add_tile(&mut self, tileset: usize, pixels: Vec<Rgba>) -> Result<u32> {
        self.tilesets
            .get_mut(tileset)
            .ok_or(Error::Index { what: "tileset" })?
            .push(pixels)
    }

    fn grid_cells(&self, tileset: usize) -> usize {
        let (cols, rows) = self.tilesets[tileset].grid_size(self.width, self.height);
        cols as usize * rows as usize
    }

    /// Clear cels for a new frame: empty grids on tilemap layers.
    fn blank_cels(&self) -> Vec<Cel> {
        let len = (self.width * self.height) as usize;
        self.layers
            .iter()
            .map(|layer| Cel {
                pixels: vec![Rgba::CLEAR; len],
                tiles: layer
                    .tileset
                    .map(|set| vec![0; self.grid_cells(usize::from(set))]),
            })
            .collect()
    }

    /// Fit every grid to a new canvas size, moving cells by whole tiles of
    /// the shift, then draw the tilemap cels again.
    fn resize_grids(&mut self, old_width: u32, old_height: u32, shift_x: i32, shift_y: i32) {
        for layer in 0..self.layers.len() {
            let Some(set) = self.layer_tileset(layer) else {
                continue;
            };
            let tileset = &self.tilesets[set];
            let (old_cols, old_rows) = tileset.grid_size(old_width, old_height);
            let (cols, rows) = tileset.grid_size(self.width, self.height);
            let dx = shift_x.div_euclid(tileset.tile_width() as i32);
            let dy = shift_y.div_euclid(tileset.tile_height() as i32);
            for frame in &mut self.frames {
                let cel = &mut frame.cels[layer];
                let old = cel.tiles.take().unwrap_or_default();
                let mut grid = vec![0; cols as usize * rows as usize];
                for row in 0..old_rows as i32 {
                    for col in 0..old_cols as i32 {
                        let (nc, nr) = (col + dx, row + dy);
                        if nc < 0 || nr < 0 || nc >= cols as i32 || nr >= rows as i32 {
                            continue;
                        }
                        grid[(nr as u32 * cols + nc as u32) as usize] = old
                            .get((row as u32 * old_cols + col as u32) as usize)
                            .copied()
                            .unwrap_or_default();
                    }
                }
                cel.tiles = Some(grid);
            }
        }
        for set in 0..self.tilesets.len() {
            self.render_tileset(set);
        }
    }

    /// Draw every cel that uses a tileset from its grid.
    fn render_tileset(&mut self, set: usize) {
        let (width, height) = (self.width, self.height);
        let tileset = &self.tilesets[set];
        for (layer, info) in self.layers.iter().enumerate() {
            if info.tileset.map(usize::from) != Some(set) {
                continue;
            }
            for frame in &mut self.frames {
                let cel = &mut frame.cels[layer];
                if let Some(grid) = &cel.tiles {
                    tileset.render(grid, width, height, &mut cel.pixels);
                }
            }
        }
    }

    /// Carry pixel edits on tilemap layers into their tiles, then draw those
    /// layers again so every copy of a changed tile shows the change. A
    /// pixel drawn on an empty cell makes a new tile of that cell. Returns
    /// how many cells could not get a tile because the tileset was full;
    /// their pixels go back to clear.
    pub(crate) fn sync_tiles(&mut self) -> usize {
        if self.layers.iter().all(|l| l.tileset.is_none()) {
            return 0;
        }
        let (width, height) = (self.width, self.height);
        // Find every change against the tiles as they are now, before any
        // tile changes, so one edited copy does not undo another.
        let mut changes: Vec<(usize, usize, Vec<usize>)> = Vec::new();
        for (f, frame) in self.frames.iter().enumerate() {
            for (l, info) in self.layers.iter().enumerate() {
                let Some(set) = info.tileset.map(usize::from) else {
                    continue;
                };
                let tileset = &self.tilesets[set];
                let cel = &frame.cels[l];
                let Some(grid) = &cel.tiles else {
                    continue;
                };
                let (cols, _) = tileset.grid_size(width, height);
                let changed: Vec<usize> = (0..cel.pixels.len())
                    .filter(|&i| {
                        let (x, y) = (i as u32 % width, i as u32 / width);
                        let cell = (y / tileset.tile_height()) * cols + x / tileset.tile_width();
                        let value = grid.get(cell as usize).copied().unwrap_or_default();
                        cel.pixels[i] != tileset.pixel(value, x, y)
                    })
                    .collect();
                if !changed.is_empty() {
                    changes.push((f, l, changed));
                }
            }
        }
        if changes.is_empty() {
            return 0;
        }
        let mut touched = vec![false; self.tilesets.len()];
        let mut full = 0;
        for (f, l, changed) in changes {
            let set = usize::from(self.layers[l].tileset.unwrap_or_default());
            touched[set] = true;
            let tileset = &mut self.tilesets[set];
            let (tw, th) = (tileset.tile_width(), tileset.tile_height());
            let (cols, _) = tileset.grid_size(width, height);
            let cel = &mut self.frames[f].cels[l];
            let Some(grid) = cel.tiles.as_mut() else {
                continue;
            };
            let mut made: Vec<u32> = Vec::new();
            for i in changed {
                let (x, y) = (i as u32 % width, i as u32 / width);
                let (col, row) = (x / tw, y / th);
                let cell = (row * cols + col) as usize;
                let value = grid[cell];
                let id = value & crate::tilemap::TILE_ID_MASK;
                if id == 0 {
                    if made.contains(&(cell as u32)) {
                        continue;
                    }
                    made.push(cell as u32);
                    // The new tile is the cell as it is drawn now.
                    let mut tile = vec![Rgba::CLEAR; tileset.tile_pixels()];
                    for dy in 0..th {
                        for dx in 0..tw {
                            let (px, py) = (col * tw + dx, row * th + dy);
                            if px < width && py < height {
                                tile[(dy * tw + dx) as usize] =
                                    cel.pixels[(py * width + px) as usize];
                            }
                        }
                    }
                    if tile.iter().all(|p| p.a == 0) {
                        continue;
                    }
                    match tileset.push(tile) {
                        Ok(new) => grid[cell] = new,
                        Err(_) => full += 1,
                    }
                } else {
                    let (sx, sy) = tileset.source(value, x % tw, y % th);
                    if let Some(tile) = tileset.tile_mut(id) {
                        tile[(sy * tw + sx) as usize] = cel.pixels[i];
                    }
                }
            }
        }
        for (set, touched) in touched.into_iter().enumerate() {
            if touched {
                self.render_tileset(set);
            }
        }
        full
    }

    /// Give a loaded document its tilesets and grids. `layer_sets` has one
    /// entry per layer; `grids[frame][layer]` is `Some` exactly on tilemap
    /// layers, sized to cover the canvas. The tilemap cels are drawn from
    /// the grids.
    pub(crate) fn attach_tilemaps(
        &mut self,
        tilesets: Vec<Tileset>,
        layer_sets: Vec<Option<u16>>,
        grids: Vec<Vec<Option<Vec<u32>>>>,
    ) -> Result<()> {
        if tilesets.len() > crate::tilemap::MAX_TILESETS
            || layer_sets.len() != self.layers.len()
            || grids.len() != self.frames.len()
        {
            return Err(Error::BadFlag("tilemap"));
        }
        for (layer, set) in self.layers.iter().zip(&layer_sets) {
            if let Some(set) = set {
                if layer.group || usize::from(*set) >= tilesets.len() {
                    return Err(Error::BadFlag("tilemap layer"));
                }
            }
        }
        for frame_grids in &grids {
            if frame_grids.len() != self.layers.len() {
                return Err(Error::BadFlag("tilemap"));
            }
            for (grid, set) in frame_grids.iter().zip(&layer_sets) {
                match (grid, set) {
                    (None, None) => {}
                    (Some(grid), Some(set)) => {
                        let tileset = &tilesets[usize::from(*set)];
                        let (cols, rows) = tileset.grid_size(self.width, self.height);
                        if grid.len() != cols as usize * rows as usize
                            || !grid.iter().all(|&v| tileset.valid_entry(v))
                        {
                            return Err(Error::BadFlag("tile grid"));
                        }
                    }
                    _ => return Err(Error::BadFlag("tile grid")),
                }
            }
        }
        self.tilesets = tilesets;
        for (layer, set) in self.layers.iter_mut().zip(layer_sets) {
            layer.tileset = set;
        }
        for (frame, frame_grids) in self.frames.iter_mut().zip(grids) {
            for (cel, grid) in frame.cels.iter_mut().zip(frame_grids) {
                cel.tiles = grid;
            }
        }
        for set in 0..self.tilesets.len() {
            self.render_tileset(set);
        }
        Ok(())
    }
}

fn mark(x: i32, y: i32, amber: Rgba, teal: Rgba) -> Vec<(i32, i32, Rgba)> {
    vec![
        (x, y, teal),
        (x + 1, y, teal),
        (x, y + 1, amber),
        (x + 1, y + 1, amber),
        (x + 2, y + 1, amber),
        (x + 1, y + 2, amber),
        (x + 3, y + 3, teal),
    ]
}

/// Original 16-color "Ink" palette. These values are not taken from another editor.
pub fn default_palette() -> Vec<Rgba> {
    const HEX: [&str; 16] = [
        "14161F", "2A2E3D", "565D73", "C5C8D4", "F6F3EC", "E8A838", "D06A2B", "B33A3A", "7A3048",
        "215E5A", "3AA17A", "8FBE4A", "3C5C9A", "6E4C93", "D47AA8", "8D5A2B",
    ];
    HEX.into_iter()
        .map(|h| Rgba::parse_hex(h).expect("static palette entry"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_is_one_clear_layer() {
        let doc = Document::new(8, 4).unwrap();
        assert_eq!(doc.width(), 8);
        assert_eq!(doc.frame_count(), 1);
        assert!(doc.composite(0).unwrap().iter().all(|p| *p == Rgba::CLEAR));
        assert!(Document::new(0, 4).is_err());
        assert!(Document::new(1025, 1).is_err());
    }

    #[test]
    fn layers_are_shared_and_pixels_are_not() {
        let mut doc = Document::new(4, 4).unwrap();
        doc.add_frame().unwrap();
        doc.add_layer("Ink").unwrap();
        doc.set_pixel(1, 1, 0, 0, Rgba::new(1, 2, 3, 255)).unwrap();
        assert_eq!(doc.cel(0, 1).unwrap()[0], Rgba::CLEAR);
        assert_eq!(doc.layer(1).unwrap().name, "Ink");
        assert_eq!(doc.frame_count(), 2);
        assert_eq!(doc.layer_count(), 2);
    }

    #[test]
    fn cannot_remove_the_last_layer_or_frame() {
        let mut doc = Document::new(2, 2).unwrap();
        assert!(matches!(doc.delete_layer(0), Err(Error::LastLayer)));
        assert!(matches!(doc.delete_frame(0), Err(Error::LastFrame)));
        assert!(matches!(doc.merge_down(0), Err(Error::BottomLayer)));
    }

    #[test]
    fn composite_uses_visibility_and_opacity() {
        let mut doc = Document::new(1, 1).unwrap();
        doc.frames[0].cels[0][0] = Rgba::new(255, 0, 0, 255);
        doc.add_layer("top").unwrap();
        doc.frames[0].cels[1][0] = Rgba::new(0, 0, 255, 255);
        doc.layers[1].opacity = 0;
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(255, 0, 0, 255));
        doc.layers[1].opacity = 255;
        doc.layers[1].visible = false;
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(255, 0, 0, 255));
        doc.layers[1].visible = true;
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(0, 0, 255, 255));
        assert_eq!(
            doc.composite_pixel(0, 0, 0),
            Some(Rgba::new(0, 0, 255, 255))
        );
        assert_eq!(doc.composite_pixel(0, 1, 0), None);
    }

    /// bg (gray), then group G holding `a` (white) and `b` (red), in a 1×1 document.
    fn grouped() -> Document {
        let mut doc = Document::new(1, 1).unwrap();
        doc.layers[0].name = "bg".into();
        doc.frames[0].cels[0][0] = Rgba::new(128, 128, 128, 255);
        doc.add_group("G").unwrap();
        doc.add_layer("a").unwrap();
        doc.frames[0].cels[2][0] = Rgba::new(255, 255, 255, 255);
        doc.indent_layer(2).unwrap();
        doc.add_layer("b").unwrap();
        doc.frames[0].cels[3][0] = Rgba::new(255, 0, 0, 255);
        doc.indent_layer(3).unwrap();
        doc
    }

    fn names(doc: &Document) -> Vec<&str> {
        doc.layers.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn groups_hold_the_layers_after_them_and_list_above_them() {
        let doc = grouped();
        assert_eq!(names(&doc), ["bg", "G", "a", "b"]);
        assert_eq!(
            doc.layers.iter().map(|l| l.depth).collect::<Vec<_>>(),
            [0, 0, 1, 1]
        );
        assert_eq!(doc.layer_parent(3), Some(1));
        assert_eq!(doc.layer_parent(1), None);
        assert_eq!(doc.layer_block_end(1), 4);
        assert_eq!(doc.layer_display_order(), [1, 3, 2, 0]);
        assert!(valid_layer_tree(&doc.layers));
    }

    #[test]
    fn a_group_is_drawn_alone_then_mixed_with_its_own_mode() {
        let mut doc = grouped();
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(255, 0, 0, 255));
        doc.layers[1].blend = BlendMode::Multiply;
        // The group's red is multiplied onto gray as one picture.
        let expected = Rgba::new(128, 0, 0, 255);
        assert_eq!(doc.composite(0).unwrap()[0], expected);
        assert_eq!(doc.composite_pixel(0, 0, 0), Some(expected));
        doc.layers[1].visible = false;
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(128, 128, 128, 255));
        assert!(!doc.layer_shown(3));
        assert!(doc.layer_shown(0));
        doc.layers[1].visible = true;
        doc.layers[1].blend = BlendMode::Normal;
        doc.layers[1].opacity = 0;
        assert_eq!(
            doc.composite_pixel(0, 0, 0),
            Some(Rgba::new(128, 128, 128, 255))
        );
    }

    #[test]
    fn moves_duplicates_and_deletes_carry_whole_groups() {
        let mut doc = grouped();
        doc.active_layer = 0;
        doc.move_layer(0, 1).unwrap();
        assert_eq!(names(&doc), ["G", "a", "b", "bg"]);
        assert_eq!(doc.active_layer, 3);
        assert!(doc.move_layer(3, 1).is_err());
        doc.move_layer(3, -1).unwrap();
        assert_eq!(names(&doc), ["bg", "G", "a", "b"]);
        assert_eq!(doc.active_layer, 0);
        // Inside the group, b and a swap places.
        doc.move_layer(3, -1).unwrap();
        assert_eq!(names(&doc), ["bg", "G", "b", "a"]);
        assert!(
            doc.move_layer(2, -1).is_err(),
            "b is the bottom of its group"
        );

        doc.duplicate_layer(1).unwrap();
        assert_eq!(names(&doc), ["bg", "G", "b", "a", "G copy", "b", "a"]);
        assert_eq!(doc.active_layer, 4);
        assert!(valid_layer_tree(&doc.layers));
        doc.delete_layer(4).unwrap();
        assert_eq!(names(&doc), ["bg", "G", "b", "a"]);

        doc.outdent_layer(3).unwrap();
        assert_eq!(names(&doc), ["bg", "G", "b", "a"]);
        assert_eq!(doc.layers[3].depth, 0);
        assert!(doc.outdent_layer(3).is_err());
        assert!(doc.indent_layer(0).is_err(), "nothing below bg");
        doc.indent_layer(3).unwrap();
        assert_eq!(doc.layers[3].depth, 1);
        assert!(valid_layer_tree(&doc.layers));
    }

    #[test]
    fn groups_cannot_be_drawn_on_merged_or_left_without_layers() {
        let mut doc = grouped();
        assert!(doc.cel_mut(0, 1).is_err());
        assert!(doc.merge_down(1).is_err(), "a group");
        assert!(doc.merge_down(2).is_err(), "nothing below a in its group");
        doc.merge_down(3).unwrap();
        assert_eq!(names(&doc), ["bg", "G", "a"]);
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(255, 0, 0, 255));

        let mut only = Document::new(1, 1).unwrap();
        only.add_group("G").unwrap();
        only.active_layer = 0;
        only.indent_layer(0).ok();
        assert!(
            only.delete_layer(0).is_err(),
            "the last drawable layer stays"
        );
        only.delete_layer(1).unwrap();
        assert_eq!(only.layer_count(), 1);
    }

    #[test]
    fn wrapping_a_layer_puts_a_group_in_its_place() {
        let mut doc = grouped();
        let before = doc.composite(0).unwrap();
        doc.wrap_in_group(0, "Back").unwrap();
        assert_eq!(names(&doc), ["Back", "bg", "G", "a", "b"]);
        assert_eq!(
            doc.layers.iter().map(|l| l.depth).collect::<Vec<_>>(),
            [0, 1, 0, 1, 1]
        );
        assert_eq!(doc.active_layer, 0);
        doc.wrap_in_group(2, "").unwrap();
        assert_eq!(names(&doc), ["Back", "bg", "Group 3", "G", "a", "b"]);
        assert_eq!(
            doc.layers.iter().map(|l| l.depth).collect::<Vec<_>>(),
            [0, 1, 0, 1, 2, 2]
        );
        assert!(valid_layer_tree(&doc.layers));
        assert_eq!(doc.composite(0).unwrap(), before);
    }

    #[test]
    fn layer_trees_are_checked() {
        let layer = |group: bool, depth: u8| LayerInfo {
            group,
            depth,
            ..LayerInfo::new("x".into())
        };
        assert!(valid_layer_tree(&[
            layer(false, 0),
            layer(true, 0),
            layer(false, 1)
        ]));
        assert!(!valid_layer_tree(&[layer(false, 1)]));
        assert!(
            !valid_layer_tree(&[layer(false, 0), layer(false, 1)]),
            "not after a group"
        );
        assert!(
            !valid_layer_tree(&[layer(true, 0), layer(false, 2)]),
            "skips a level"
        );
        assert!(!valid_layer_tree(&[layer(true, 0)]), "no drawable layer");
        assert!(valid_layer_tree(&[
            layer(true, 0),
            layer(false, 1),
            layer(false, 0)
        ]));
    }

    #[test]
    fn layers_composite_and_merge_with_their_blend_mode() {
        let mut doc = Document::new(1, 1).unwrap();
        doc.frames[0].cels[0][0] = Rgba::new(128, 128, 128, 255);
        doc.add_layer("tint").unwrap();
        doc.frames[0].cels[1][0] = Rgba::new(255, 0, 0, 255);
        doc.layers[1].blend = BlendMode::Multiply;
        let expected = Rgba::new(128, 0, 0, 255);
        assert_eq!(doc.composite(0).unwrap()[0], expected);
        assert_eq!(doc.composite_pixel(0, 0, 0), Some(expected));
        doc.merge_down(1).unwrap();
        assert_eq!(doc.layer_count(), 1);
        assert_eq!(doc.composite(0).unwrap()[0], expected);
    }

    #[test]
    fn merge_down_bakes_both_opacities() {
        let mut doc = Document::new(1, 1).unwrap();
        doc.frames[0].cels[0][0] = Rgba::new(255, 0, 0, 255);
        doc.add_layer("top").unwrap();
        doc.frames[0].cels[1][0] = Rgba::new(0, 0, 255, 255);
        doc.layers[1].opacity = 255;
        doc.merge_down(1).unwrap();
        assert_eq!(doc.layer_count(), 1);
        assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(0, 0, 255, 255));
    }

    #[test]
    fn rejects_an_oversized_stack() {
        let mut doc = Document::new(2, 2).unwrap();
        for _ in 0..(crate::limits::MAX_LAYERS - 1) {
            doc.add_layer("x").unwrap();
        }
        let err = doc.add_layer("overflow").unwrap_err();
        assert!(matches!(err, Error::Limit { .. }));
    }

    #[test]
    fn demo_has_two_frames() {
        let doc = Document::demo().unwrap();
        assert_eq!(doc.frame_count(), 2);
        assert_eq!(doc.layer_count(), 2);
        assert_ne!(doc.cel(0, 1).unwrap(), doc.cel(1, 1).unwrap());
    }

    #[test]
    fn resize_shifts_pixels_and_crops_the_rest() {
        let mut doc = Document::new(4, 3).unwrap();
        let red = Rgba::new(200, 0, 0, 255);
        let blue = Rgba::new(0, 0, 200, 255);
        doc.set_pixel(0, 0, 0, 0, red).unwrap();
        doc.set_pixel(0, 0, 3, 2, blue).unwrap();
        doc.add_frame().unwrap();
        doc.set_pixel(1, 0, 0, 0, blue).unwrap();
        doc.resize(3, 3, 1, 1).unwrap();
        assert_eq!((doc.width(), doc.height()), (3, 3));
        assert_eq!(doc.cel(0, 0).unwrap()[1 + 3], red);
        assert_eq!(doc.cel(0, 0).unwrap()[0], Rgba::CLEAR);
        assert!(doc.cel(0, 0).unwrap().iter().all(|px| *px != blue));
        assert_eq!(doc.cel(1, 0).unwrap()[1 + 3], blue);
        assert!(doc.resize(0, 3, 0, 0).is_err());
        assert_eq!(doc.width(), 3);
    }
}
