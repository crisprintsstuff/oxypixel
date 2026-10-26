//! Pack sprites from many files into one atlas PNG with a JSON description.
//!
//! The packer is a shelf packer: sprites are sorted tallest first and placed
//! left to right in rows no wider than the width limit. That keeps the layout
//! deterministic for the same inputs, which matters when the atlas is checked
//! into a game's repository.

use std::collections::HashMap;
use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
pub use oxypixel_core::FrameTag;
use oxypixel_core::{
    frame_tags_json, Document, Rgba, DEFAULT_FRAME_MS, MAX_SHEET_PADDING, MAX_SHEET_PIXELS,
    MAX_SHEET_WIDTH,
};

/// Why a pack failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("there are no sprites to pack")]
    NothingToPack,
    #[error("sprite {name} is {width} pixels wide, wider than the atlas limit of {max}")]
    SpriteTooWide { name: String, width: u32, max: u32 },
    #[error("sprite {0} has the wrong number of pixels for its size")]
    BadSprite(String),
    #[error("{what} is {value}, and the limit is {max}")]
    Limit {
        what: &'static str,
        value: u64,
        max: u64,
    },
    #[error("{0}")]
    Core(#[from] oxypixel_core::Error),
    #[error("could not write the PNG: {0}")]
    Png(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Straight RGBA8 bytes, row by row.
fn rgba_bytes(pixels: &[Rgba]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|px| [px.r, px.g, px.b, px.a])
        .collect()
}

/// A JSON string literal, with quotes, backslashes, and control characters escaped.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One image to place in the atlas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Straight-alpha pixels, row by row.
    pub pixels: Vec<Rgba>,
    /// Frame duration in milliseconds, written to the JSON.
    pub duration: u32,
}

/// How sprites are placed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Packer {
    /// Rows, tallest sprite first. Simple and stable; the default since 0.7.5.
    #[default]
    Shelf,
    /// MaxRects with the bottom-left rule: each sprite goes where its bottom
    /// edge is highest, then furthest left. Fills the gaps a shelf leaves.
    MaxRects,
}

/// How the atlas is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackOptions {
    pub packer: Packer,
    /// Clear pixels between neighboring sprites. Not added at the atlas edge.
    pub padding: u32,
    /// Crop each sprite to its non-clear pixels. The JSON records the offset.
    pub trim: bool,
    /// Sprites with the same pixels (after trimming) share one rectangle.
    pub dedupe: bool,
    /// No row is wider than this.
    pub max_width: u32,
    /// Round the atlas width and height up to powers of two.
    pub power_of_two: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            packer: Packer::Shelf,
            padding: 1,
            trim: false,
            dedupe: false,
            max_width: 2048,
            power_of_two: false,
        }
    }
}

/// Where one sprite went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub name: String,
    /// The rectangle in the atlas.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Where that rectangle sits in the untrimmed sprite.
    pub offset_x: u32,
    pub offset_y: u32,
    /// The sprite's size before trimming.
    pub source_width: u32,
    pub source_height: u32,
    pub duration: u32,
}

impl Placement {
    pub fn trimmed(&self) -> bool {
        (self.width, self.height) != (self.source_width, self.source_height)
    }
}

/// The tags of one file's sprites, moved to where those sprites start in the
/// whole list, so `from` and `to` index the packed sprites (the order of
/// `frames` in the JSON).
pub fn frame_tags(tags: &[FrameTag], first_sprite: usize) -> Vec<FrameTag> {
    tags.iter()
        .map(|tag| FrameTag {
            from: first_sprite + tag.from,
            to: first_sprite + tag.to,
            ..tag.clone()
        })
        .collect()
}

/// A packed atlas: the pixels and where each input sprite is, in input order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgba>,
    pub placements: Vec<Placement>,
}

/// Every frame of `doc`, composited, as sprites named after `stem`.
///
/// A one-frame document gives one sprite called `stem`. Frames of a longer
/// one are `stem 0`, `stem 1`, and so on, as Aseprite names them.
pub fn sprites_from_document(doc: &Document, stem: &str) -> Result<Vec<Sprite>> {
    let count = doc.frame_count();
    (0..count)
        .map(|frame| {
            Ok(Sprite {
                name: frame_name(stem, frame, count),
                width: doc.width(),
                height: doc.height(),
                pixels: doc.composite(frame)?,
                duration: doc.frame_duration(frame).unwrap_or(DEFAULT_FRAME_MS),
            })
        })
        .collect()
}

/// `stem` for a one-frame file, `stem N` for frame N of a longer one.
pub fn frame_name(stem: &str, frame: usize, count: usize) -> String {
    if count == 1 {
        stem.to_string()
    } else {
        format!("{stem} {frame}")
    }
}

/// Every frame of a decoded `.aseprite` file as sprites named after `stem`.
pub fn sprites_from_aseprite(file: &crate::aseprite::AseFile, stem: &str) -> Vec<Sprite> {
    let count = file.frames.len();
    file.frames
        .iter()
        .enumerate()
        .map(|(frame, (pixels, duration))| Sprite {
            name: frame_name(stem, frame, count),
            width: file.width,
            height: file.height,
            pixels: pixels.clone(),
            duration: *duration,
        })
        .collect()
}

/// Give each sprite a name no earlier sprite has, by adding ` (2)`, ` (3)`, and so on.
pub fn unique_names(sprites: &mut [Sprite]) {
    let mut taken: HashMap<String, u32> = HashMap::new();
    for sprite in sprites.iter() {
        taken.entry(sprite.name.clone()).or_insert(0);
    }
    let mut seen: HashMap<String, ()> = HashMap::new();
    for sprite in sprites.iter_mut() {
        if seen.insert(sprite.name.clone(), ()).is_none() {
            continue;
        }
        let base = sprite.name.clone();
        let mut n = 2;
        let name = loop {
            let candidate = format!("{base} ({n})");
            if !taken.contains_key(&candidate) && !seen.contains_key(&candidate) {
                break candidate;
            }
            n += 1;
        };
        seen.insert(name.clone(), ());
        sprite.name = name;
    }
}

/// The smallest rectangle holding every non-clear pixel. A clear sprite keeps
/// one pixel at its top left, so it still has a rectangle in the atlas.
fn trim_bounds(sprite: &Sprite) -> (u32, u32, u32, u32) {
    let w = sprite.width as usize;
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for (i, _) in sprite.pixels.iter().enumerate().filter(|(_, px)| px.a > 0) {
        let (x, y) = (i % w, i / w);
        left = left.min(x);
        right = right.max(x);
        top = top.min(y);
        bottom = bottom.max(y);
    }
    if left == usize::MAX {
        return (0, 0, 1, 1);
    }
    (
        left as u32,
        top as u32,
        (right - left + 1) as u32,
        (bottom - top + 1) as u32,
    )
}

fn crop(sprite: &Sprite, x: u32, y: u32, w: u32, h: u32) -> Vec<Rgba> {
    let src_w = sprite.width as usize;
    let mut out = Vec::with_capacity(w as usize * h as usize);
    for row in y..y + h {
        let start = row as usize * src_w + x as usize;
        out.extend_from_slice(&sprite.pixels[start..start + w as usize]);
    }
    out
}

/// One rectangle to place: the cropped pixels shared by one or more sprites.
struct Cell {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
    x: u32,
    y: u32,
}

fn round_up_pow2(value: u32) -> u32 {
    value.checked_next_power_of_two().unwrap_or(value)
}

/// Where each rectangle goes, and the width and height they use.
type Layout = (Vec<(u32, u32)>, u32, u32);

fn too_tall(height: u64) -> Error {
    Error::Limit {
        what: "atlas height",
        value: height,
        max: u64::from(MAX_SHEET_WIDTH),
    }
}

/// Rows no wider than `max_width`, tallest first, then widest, then input
/// order, so the same sizes always give the same layout.
fn place_shelves(sizes: &[(u32, u32)], padding: u32, max_width: u32) -> Result<Layout> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| (sizes[b].1, sizes[b].0, a).cmp(&(sizes[a].1, sizes[a].0, b)));
    let pad = u64::from(padding);
    let limit = u64::from(max_width);
    let mut spots = vec![(0, 0); sizes.len()];
    let (mut x, mut y, mut shelf_h, mut used_w) = (0u64, 0u64, 0u64, 0u64);
    for &index in &order {
        let (w, h) = (u64::from(sizes[index].0), u64::from(sizes[index].1));
        if x > 0 && x + w > limit {
            y += shelf_h + pad;
            x = 0;
            shelf_h = 0;
        }
        spots[index] = (x as u32, y as u32);
        used_w = used_w.max(x + w);
        shelf_h = shelf_h.max(h);
        x += w + pad;
        if y + shelf_h > u64::from(MAX_SHEET_WIDTH) {
            return Err(too_tall(y + shelf_h));
        }
    }
    Ok((spots, used_w as u32, (y + shelf_h) as u32))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Free {
    x: u64,
    y: u64,
    w: u64,
    h: u64,
}

impl Free {
    fn contains(&self, other: &Free) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.w <= self.x + self.w
            && other.y + other.h <= self.y + self.h
    }
}

/// MaxRects, bottom-left rule, in a bin `max_width` wide and as tall as the
/// sheet cap. Each rectangle is grown by the padding on its right and bottom,
/// and the bin by the same amount, so padding only falls between sprites.
/// Tallest first, then widest, input order breaking ties, as the shelf
/// sorts; among equal spots the lowest, then leftmost, wins, so the result
/// is deterministic.
fn place_maxrects(sizes: &[(u32, u32)], padding: u32, max_width: u32) -> Result<Layout> {
    let pad = u64::from(padding);
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| (sizes[b].1, sizes[b].0, a).cmp(&(sizes[a].1, sizes[a].0, b)));
    let mut free = vec![Free {
        x: 0,
        y: 0,
        w: u64::from(max_width) + pad,
        h: u64::from(MAX_SHEET_WIDTH) + pad,
    }];
    let mut spots = vec![(0, 0); sizes.len()];
    let (mut used_w, mut used_h) = (0u64, 0u64);
    for &index in &order {
        let (w, h) = (
            u64::from(sizes[index].0) + pad,
            u64::from(sizes[index].1) + pad,
        );
        let best = free
            .iter()
            .filter(|f| w <= f.w && h <= f.h)
            .min_by_key(|f| (f.y + h, f.x))
            .copied();
        let Some(spot) = best else {
            return Err(too_tall(used_h + h));
        };
        let placed = Free {
            x: spot.x,
            y: spot.y,
            w,
            h,
        };
        spots[index] = (placed.x as u32, placed.y as u32);
        used_w = used_w.max(placed.x + w - pad);
        used_h = used_h.max(placed.y + h - pad);
        free = split_free(free, &placed);
    }
    Ok((spots, used_w as u32, used_h as u32))
}

/// Cut `placed` out of every free rectangle, then drop rectangles that sit
/// inside another one.
fn split_free(free: Vec<Free>, placed: &Free) -> Vec<Free> {
    let mut next = Vec::with_capacity(free.len() + 4);
    for f in free {
        let overlaps = placed.x < f.x + f.w
            && placed.x + placed.w > f.x
            && placed.y < f.y + f.h
            && placed.y + placed.h > f.y;
        if !overlaps {
            next.push(f);
            continue;
        }
        if placed.x > f.x {
            next.push(Free {
                w: placed.x - f.x,
                ..f
            });
        }
        if placed.x + placed.w < f.x + f.w {
            next.push(Free {
                x: placed.x + placed.w,
                w: f.x + f.w - (placed.x + placed.w),
                ..f
            });
        }
        if placed.y > f.y {
            next.push(Free {
                h: placed.y - f.y,
                ..f
            });
        }
        if placed.y + placed.h < f.y + f.h {
            next.push(Free {
                y: placed.y + placed.h,
                h: f.y + f.h - (placed.y + placed.h),
                ..f
            });
        }
    }
    let mut kept: Vec<Free> = Vec::with_capacity(next.len());
    for (i, f) in next.iter().enumerate() {
        let inside = next
            .iter()
            .enumerate()
            .any(|(j, g)| i != j && g.contains(f) && (g != f || j < i));
        if !inside {
            kept.push(*f);
        }
    }
    kept
}

/// Pack `sprites` into one atlas. Placements come back in input order.
pub fn pack_sprites(sprites: &[Sprite], options: PackOptions) -> Result<Atlas> {
    if sprites.is_empty() {
        return Err(Error::NothingToPack);
    }
    if options.padding > MAX_SHEET_PADDING {
        return Err(Error::Limit {
            what: "atlas padding",
            value: u64::from(options.padding),
            max: u64::from(MAX_SHEET_PADDING),
        });
    }
    let max_width = options.max_width.clamp(1, MAX_SHEET_WIDTH);
    let mut cells: Vec<Cell> = Vec::new();
    let mut seen: HashMap<(u32, u32, Vec<u8>), usize> = HashMap::new();
    // For each sprite: its cell and its trim rectangle.
    let mut owners = Vec::with_capacity(sprites.len());
    for sprite in sprites {
        let expected = sprite.width as usize * sprite.height as usize;
        if sprite.width == 0 || sprite.height == 0 || sprite.pixels.len() != expected {
            return Err(Error::BadSprite(sprite.name.clone()));
        }
        let (tx, ty, tw, th) = if options.trim {
            trim_bounds(sprite)
        } else {
            (0, 0, sprite.width, sprite.height)
        };
        if tw > max_width {
            return Err(Error::SpriteTooWide {
                name: sprite.name.clone(),
                width: tw,
                max: max_width,
            });
        }
        let pixels = crop(sprite, tx, ty, tw, th);
        let key = (tw, th, rgba_bytes(&pixels));
        let cell = match options.dedupe.then(|| seen.get(&key).copied()).flatten() {
            Some(index) => index,
            None => {
                cells.push(Cell {
                    width: tw,
                    height: th,
                    pixels,
                    x: 0,
                    y: 0,
                });
                if options.dedupe {
                    seen.insert(key, cells.len() - 1);
                }
                cells.len() - 1
            }
        };
        owners.push((cell, tx, ty));
    }

    let sizes: Vec<(u32, u32)> = cells.iter().map(|c| (c.width, c.height)).collect();
    let (spots, used_w, used_h) = match options.packer {
        Packer::Shelf => place_shelves(&sizes, options.padding, max_width)?,
        Packer::MaxRects => place_maxrects(&sizes, options.padding, max_width)?,
    };
    for (cell, (x, y)) in cells.iter_mut().zip(spots) {
        cell.x = x;
        cell.y = y;
    }
    let (mut width, mut height) = (used_w, used_h);
    if options.power_of_two {
        width = round_up_pow2(width);
        height = round_up_pow2(height);
    }
    let area = u64::from(width) * u64::from(height);
    if area > MAX_SHEET_PIXELS {
        return Err(Error::Limit {
            what: "atlas pixels",
            value: area,
            max: MAX_SHEET_PIXELS,
        });
    }

    let atlas_w = width as usize;
    let mut out = vec![Rgba::CLEAR; atlas_w * height as usize];
    for cell in &cells {
        for (row, line) in cell.pixels.chunks_exact(cell.width as usize).enumerate() {
            let start = (cell.y as usize + row) * atlas_w + cell.x as usize;
            out[start..start + line.len()].copy_from_slice(line);
        }
    }
    let placements = sprites
        .iter()
        .zip(owners)
        .map(|(sprite, (index, tx, ty))| {
            let cell = &cells[index];
            Placement {
                name: sprite.name.clone(),
                x: cell.x,
                y: cell.y,
                width: cell.width,
                height: cell.height,
                offset_x: tx,
                offset_y: ty,
                source_width: sprite.width,
                source_height: sprite.height,
                duration: sprite.duration,
            }
        })
        .collect();
    Ok(Atlas {
        width,
        height,
        pixels: out,
        placements,
    })
}

/// The atlas as a PNG.
pub fn atlas_png_bytes(atlas: &Atlas) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    PngEncoder::new(&mut cursor)
        .write_image(
            &rgba_bytes(&atlas.pixels),
            atlas.width,
            atlas.height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|err| Error::Png(err.to_string()))?;
    Ok(cursor.into_inner())
}

/// The atlas description as a JSON hash: `frames` maps each sprite name to its
/// rectangle, trim offset, source size, and duration, then `meta` names the
/// image. Phaser, PixiJS, Godot importers, and Aseprite's own exporter use
/// this layout. Names must be unique; see [`unique_names`]. `tags` become
/// `meta.frameTags`, as Aseprite writes them; the list is empty when no
/// input had tags.
pub fn atlas_json(atlas: &Atlas, image: &str, tags: &[FrameTag]) -> String {
    let mut out = String::from("{\n  \"frames\": {\n");
    for (index, p) in atlas.placements.iter().enumerate() {
        out.push_str(&format!(
            "    {}: {{\"frame\": {{\"x\": {}, \"y\": {}, \"w\": {}, \"h\": {}}}, \"rotated\": false, \"trimmed\": {}, \"spriteSourceSize\": {{\"x\": {}, \"y\": {}, \"w\": {}, \"h\": {}}}, \"sourceSize\": {{\"w\": {}, \"h\": {}}}, \"duration\": {}}}",
            json_string(&p.name),
            p.x,
            p.y,
            p.width,
            p.height,
            p.trimmed(),
            p.offset_x,
            p.offset_y,
            p.width,
            p.height,
            p.source_width,
            p.source_height,
            p.duration,
        ));
        out.push_str(if index + 1 < atlas.placements.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    out.push_str(&format!(
        "  }},\n  \"meta\": {{\"app\": \"OxyPack\", \"version\": \"{}\", \"image\": {}, \"format\": \"RGBA8888\", \"size\": {{\"w\": {}, \"h\": {}}}, \"scale\": \"1\", \"frameTags\": [{}]}}\n}}\n",
        env!("CARGO_PKG_VERSION"),
        json_string(image),
        atlas.width,
        atlas.height,
        frame_tags_json(tags),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(name: &str, w: u32, h: u32, n: u8) -> Sprite {
        Sprite {
            name: name.to_string(),
            width: w,
            height: h,
            pixels: vec![Rgba::new(n, 0, 0, 255); (w * h) as usize],
            duration: 100,
        }
    }

    fn at(atlas: &Atlas, x: u32, y: u32) -> Rgba {
        atlas.pixels[(y * atlas.width + x) as usize]
    }

    #[test]
    fn shelves_fill_tallest_first_and_wrap_at_the_width_limit() {
        let sprites = [
            solid("a", 2, 2, 1),
            solid("b", 3, 4, 2),
            solid("c", 2, 3, 3),
        ];
        let options = PackOptions {
            padding: 1,
            max_width: 6,
            ..PackOptions::default()
        };
        let atlas = pack_sprites(&sprites, options).unwrap();
        // Row 0: b (3x4) then c (2x3) at x 4. Row 1: a at y 5.
        let place = |name: &str| atlas.placements.iter().find(|p| p.name == name).unwrap();
        assert_eq!((place("b").x, place("b").y), (0, 0));
        assert_eq!((place("c").x, place("c").y), (4, 0));
        assert_eq!((place("a").x, place("a").y), (0, 5));
        assert_eq!((atlas.width, atlas.height), (6, 7));
        assert_eq!(at(&atlas, 4, 0), Rgba::new(3, 0, 0, 255));
        assert_eq!(at(&atlas, 3, 0), Rgba::CLEAR);
        // Placements stay in input order.
        let names: Vec<_> = atlas.placements.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    /// No two placed rectangles overlap or come closer than `pad`, and all
    /// fit inside the atlas.
    fn assert_apart(atlas: &Atlas, pad: u32) {
        let rects: Vec<_> = atlas
            .placements
            .iter()
            .map(|p| (p.x, p.y, p.width, p.height))
            .collect();
        for (i, a) in rects.iter().enumerate() {
            assert!(
                a.0 + a.2 <= atlas.width && a.1 + a.3 <= atlas.height,
                "{a:?} outside"
            );
            for b in &rects[i + 1..] {
                let apart = a.0 + a.2 + pad <= b.0
                    || b.0 + b.2 + pad <= a.0
                    || a.1 + a.3 + pad <= b.1
                    || b.1 + b.3 + pad <= a.1;
                assert!(apart, "{a:?} and {b:?} closer than {pad}");
            }
        }
    }

    #[test]
    fn maxrects_fills_the_gap_a_shelf_leaves() {
        // A 12-wide atlas: the shelf puts tall + two squares on row one and the
        // last two squares on a second row; MaxRects stacks them beside tall.
        let sprites = [
            solid("tall", 4, 8, 1),
            solid("a", 4, 4, 2),
            solid("b", 4, 4, 3),
            solid("c", 4, 4, 4),
            solid("d", 4, 4, 5),
        ];
        let base = PackOptions {
            padding: 0,
            max_width: 12,
            ..PackOptions::default()
        };
        let shelf = pack_sprites(&sprites, base).unwrap();
        let rects = PackOptions {
            packer: Packer::MaxRects,
            ..base
        };
        let packed = pack_sprites(&sprites, rects).unwrap();
        assert_eq!((shelf.width, shelf.height), (12, 12));
        assert_eq!((packed.width, packed.height), (12, 8));
        assert_apart(&packed, 0);
        // The pixels land where the placements say.
        for p in &packed.placements {
            let n = sprites.iter().find(|s| s.name == p.name).unwrap().pixels[0];
            assert_eq!(at(&packed, p.x, p.y), n);
        }
        // Same inputs, same atlas.
        assert_eq!(pack_sprites(&sprites, rects).unwrap(), packed);
    }

    #[test]
    fn maxrects_keeps_padding_and_the_width_limit_for_mixed_sizes() {
        let mut sprites = Vec::new();
        let mut seed = 7u32;
        for i in 0..60 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let w = 1 + (seed >> 16) % 13;
            let h = 1 + (seed >> 8) % 11;
            sprites.push(solid(&format!("s{i}"), w, h, (i % 250) as u8 + 1));
        }
        for padding in [0, 1, 3] {
            let options = PackOptions {
                packer: Packer::MaxRects,
                padding,
                max_width: 40,
                ..PackOptions::default()
            };
            let atlas = pack_sprites(&sprites, options).unwrap();
            assert!(atlas.width <= 40);
            assert_apart(&atlas, padding);
            let shelf = pack_sprites(
                &sprites,
                PackOptions {
                    packer: Packer::Shelf,
                    ..options
                },
            )
            .unwrap();
            assert!(atlas.height <= shelf.height, "padding {padding}");
        }
        // A sprite wider than the limit is still an error.
        let wide = PackOptions {
            packer: Packer::MaxRects,
            max_width: 4,
            ..PackOptions::default()
        };
        assert!(pack_sprites(&[solid("big", 5, 1, 1)], wide).is_err());
    }

    #[test]
    fn trim_crops_to_the_ink_and_records_the_offset() {
        let mut sprite = Sprite {
            name: "dot".into(),
            width: 4,
            height: 3,
            pixels: vec![Rgba::CLEAR; 12],
            duration: 80,
        };
        sprite.pixels[4 + 1] = Rgba::new(9, 9, 9, 255);
        sprite.pixels[4 + 2] = Rgba::new(8, 8, 8, 255);
        let options = PackOptions {
            trim: true,
            ..PackOptions::default()
        };
        let atlas = pack_sprites(&[sprite.clone()], options).unwrap();
        let p = &atlas.placements[0];
        assert_eq!((p.width, p.height, p.offset_x, p.offset_y), (2, 1, 1, 1));
        assert_eq!((p.source_width, p.source_height), (4, 3));
        assert!(p.trimmed());
        assert_eq!((atlas.width, atlas.height), (2, 1));
        assert_eq!(at(&atlas, 0, 0), Rgba::new(9, 9, 9, 255));

        // A clear sprite keeps one pixel.
        sprite.pixels = vec![Rgba::CLEAR; 12];
        let atlas = pack_sprites(&[sprite], options).unwrap();
        assert_eq!(
            (atlas.placements[0].width, atlas.placements[0].height),
            (1, 1)
        );
    }

    #[test]
    fn dedupe_shares_a_rectangle_and_pot_rounds_up() {
        let sprites = [
            solid("a", 3, 3, 1),
            solid("b", 3, 3, 1),
            solid("c", 3, 3, 2),
        ];
        let options = PackOptions {
            dedupe: true,
            power_of_two: true,
            ..PackOptions::default()
        };
        let atlas = pack_sprites(&sprites, options).unwrap();
        let [a, b, c] = &atlas.placements[..] else {
            panic!("three placements")
        };
        assert_eq!((a.x, a.y), (b.x, b.y));
        assert_ne!((a.x, a.y), (c.x, c.y));
        // Two 3-wide cells and one padding pixel: 7, rounded to 8. Height 3 to 4.
        assert_eq!((atlas.width, atlas.height), (8, 4));
        let plain = pack_sprites(&sprites, PackOptions::default()).unwrap();
        assert_eq!((plain.width, plain.height), (11, 3));
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(matches!(
            pack_sprites(&[], PackOptions::default()),
            Err(Error::NothingToPack)
        ));
        let wide = PackOptions {
            max_width: 4,
            ..PackOptions::default()
        };
        assert!(matches!(
            pack_sprites(&[solid("big", 5, 1, 1)], wide),
            Err(Error::SpriteTooWide {
                width: 5,
                max: 4,
                ..
            })
        ));
        let mut short = solid("short", 2, 2, 1);
        short.pixels.pop();
        assert!(pack_sprites(&[short], PackOptions::default()).is_err());
        let padded = PackOptions {
            padding: MAX_SHEET_PADDING + 1,
            ..PackOptions::default()
        };
        assert!(pack_sprites(&[solid("a", 1, 1, 1)], padded).is_err());
    }

    #[test]
    fn names_are_made_unique_and_frames_are_numbered() {
        let mut sprites = vec![
            solid("hero", 1, 1, 1),
            solid("hero", 1, 1, 2),
            solid("hero (2)", 1, 1, 3),
            solid("hero", 1, 1, 4),
        ];
        unique_names(&mut sprites);
        let names: Vec<_> = sprites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["hero", "hero (3)", "hero (2)", "hero (4)"]);

        let mut doc = Document::new(2, 2).unwrap();
        assert_eq!(sprites_from_document(&doc, "idle").unwrap()[0].name, "idle");
        doc.add_frame().unwrap();
        doc.set_frame_duration(1, 250).unwrap();
        let frames = sprites_from_document(&doc, "walk").unwrap();
        assert_eq!(frames[0].name, "walk 0");
        assert_eq!(
            (frames[1].name.as_str(), frames[1].duration),
            ("walk 1", 250)
        );
    }

    #[test]
    fn json_hash_lists_each_sprite_and_the_png_decodes() {
        let options = PackOptions {
            trim: true,
            ..PackOptions::default()
        };
        let mut dot = solid("dot \"x\"", 3, 3, 5);
        dot.pixels[0] = Rgba::CLEAR;
        let atlas = pack_sprites(&[solid("a", 2, 2, 1), dot], options).unwrap();
        let text = atlas_json(&atlas, "atlas.png", &[]);
        assert!(text.contains("\"frameTags\": []"));
        let tags = frame_tags(
            &[
                FrameTag {
                    name: "run \"fast\"".into(),
                    from: 0,
                    to: 1,
                    direction: oxypixel_core::LoopDirection::PingPong,
                    repeat: 2,
                },
                FrameTag {
                    name: "idle".into(),
                    from: 1,
                    to: 1,
                    direction: oxypixel_core::LoopDirection::Reverse,
                    repeat: 0,
                },
            ],
            5,
        );
        assert_eq!((tags[0].from, tags[0].to, tags[1].from), (5, 6, 6));
        let text = atlas_json(&atlas, "atlas.png", &tags);
        assert!(text.contains(
            "\"frameTags\": [{\"name\": \"run \\\"fast\\\"\", \"from\": 5, \"to\": 6, \"direction\": \"pingpong\", \"repeat\": \"2\"}, {\"name\": \"idle\", \"from\": 6, \"to\": 6, \"direction\": \"reverse\"}]"
        ), "{text}");
        assert!(text.contains("\"a\": {\"frame\": {\"x\": 4, \"y\": 0, \"w\": 2, \"h\": 2}"));
        assert!(text.contains("\"dot \\\"x\\\"\": {\"frame\": {\"x\": 0, \"y\": 0, \"w\": 3, \"h\": 3}, \"rotated\": false, \"trimmed\": false"));
        assert!(text.contains("\"image\": \"atlas.png\""));
        assert!(text.contains("\"size\": {\"w\": 6, \"h\": 3}"));
        let png = atlas_png_bytes(&atlas).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (6, 3));
        assert_eq!(decoded.as_raw(), &rgba_bytes(&atlas.pixels));
    }
}
