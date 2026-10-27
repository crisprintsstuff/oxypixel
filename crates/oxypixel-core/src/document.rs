//! Pixel document: a shared layer stack and one cel per layer per frame.
//!
//! Layers are properties (name, visibility, opacity) shared by every frame.
//! Each frame stores its own pixels for those layers, plus a duration.

use crate::blend::{blend, BlendMode};
use crate::color::{source_over, Rgba};
use crate::error::{Error, Result};
use crate::limits::{
    check_stack, clamp_duration, clamp_name, DEFAULT_FRAME_MS, MAX_PALETTE, MAX_TAGS,
};
use crate::tags::{self, FrameTag};

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub duration_ms: u32,
    /// Empty when the frame has no name. Saved in `.oxypix` version 2.
    pub name: String,
    /// One buffer per layer, row-major, length `width * height`.
    pub cels: Vec<Vec<Rgba>>,
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
}

impl Document {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        check_stack(width, height, 1, 1)?;
        Ok(Self {
            width,
            height,
            layers: vec![LayerInfo {
                name: "Layer 1".to_string(),
                visible: true,
                opacity: 255,
                blend: BlendMode::Normal,
            }],
            frames: vec![Frame {
                duration_ms: DEFAULT_FRAME_MS,
                name: String::new(),
                cels: vec![vec![Rgba::CLEAR; (width * height) as usize]],
            }],
            active_frame: 0,
            active_layer: 0,
            palette: default_palette(),
            tags: Vec::new(),
        })
    }

    /// A two-frame sample used by File → Open example and by tests.
    pub fn demo() -> Result<Self> {
        let mut doc = Self::new(32, 32)?;
        doc.layers[0].name = "Paper".to_string();
        for px in &mut doc.frames[0].cels[0] {
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
        self.frames.len() * self.layers.len() * (self.width as usize) * (self.height as usize) * 4
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

    pub(crate) fn cel(&self, frame: usize, layer: usize) -> Result<&[Rgba]> {
        self.frames
            .get(frame)
            .and_then(|f| f.cels.get(layer))
            .map(Vec::as_slice)
            .ok_or(Error::Index { what: "cel" })
    }

    pub(crate) fn cel_mut(&mut self, frame: usize, layer: usize) -> Result<&mut [Rgba]> {
        self.frames
            .get_mut(frame)
            .and_then(|f| f.cels.get_mut(layer))
            .map(Vec::as_mut_slice)
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
        self.layers.push(LayerInfo {
            name,
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
        });
        let len = (self.width * self.height) as usize;
        for frame in &mut self.frames {
            frame.cels.push(vec![Rgba::CLEAR; len]);
        }
        self.active_layer = self.layers.len() - 1;
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
        let mut info = self.layers[index].clone();
        info.name = clamp_name(&format!("{} copy", info.name));
        self.layers.insert(index + 1, info);
        for frame in &mut self.frames {
            let copy = frame.cels[index].clone();
            frame.cels.insert(index + 1, copy);
        }
        self.active_layer = index + 1;
        Ok(())
    }

    pub fn delete_layer(&mut self, index: usize) -> Result<()> {
        if self.layers.len() == 1 {
            return Err(Error::LastLayer);
        }
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        self.layers.remove(index);
        for frame in &mut self.frames {
            frame.cels.remove(index);
        }
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        } else if self.active_layer > index {
            self.active_layer -= 1;
        }
        Ok(())
    }

    /// Move a layer by `delta` positions. Positive is toward the top of the stack.
    pub fn move_layer(&mut self, index: usize, delta: i32) -> Result<()> {
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
        let target = index as i32 + delta;
        if target < 0 || target >= self.layers.len() as i32 {
            return Err(Error::Index { what: "layer" });
        }
        let target = target as usize;
        self.layers.swap(index, target);
        for frame in &mut self.frames {
            frame.cels.swap(index, target);
        }
        if self.active_layer == index {
            self.active_layer = target;
        } else if self.active_layer == target {
            self.active_layer = index;
        }
        Ok(())
    }

    /// Bake `index` onto the layer below on every frame, then remove it.
    pub fn merge_down(&mut self, index: usize) -> Result<()> {
        if index == 0 {
            return Err(Error::BottomLayer);
        }
        if index >= self.layers.len() {
            return Err(Error::Index { what: "layer" });
        }
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
        let len = (self.width * self.height) as usize;
        let cels = vec![vec![Rgba::CLEAR; len]; self.layers.len()];
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
                *cel = next;
            }
        }
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Composite one frame, bottom layer first. Hidden layers are skipped.
    pub fn composite(&self, frame: usize) -> Result<Vec<Rgba>> {
        let frame = self
            .frames
            .get(frame)
            .ok_or(Error::Index { what: "frame" })?;
        let mut out = vec![Rgba::CLEAR; (self.width * self.height) as usize];
        for (info, cel) in self.layers.iter().zip(frame.cels.iter()) {
            if !info.visible || info.opacity == 0 {
                continue;
            }
            for (dst, src) in out.iter_mut().zip(cel.iter()) {
                *dst = blend(*dst, *src, info.opacity, info.blend);
            }
        }
        Ok(out)
    }

    /// The composited color of one pixel, using the same rules as [`Self::composite`].
    pub fn composite_pixel(&self, frame: usize, x: u32, y: u32) -> Option<Rgba> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let frame = self.frames.get(frame)?;
        let index = (y * self.width + x) as usize;
        let mut color = Rgba::CLEAR;
        for (info, cel) in self.layers.iter().zip(frame.cels.iter()) {
            if !info.visible || info.opacity == 0 {
                continue;
            }
            color = blend(color, *cel.get(index)?, info.opacity, info.blend);
        }
        Some(color)
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
        })
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
