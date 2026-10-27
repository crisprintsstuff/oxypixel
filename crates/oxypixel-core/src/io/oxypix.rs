//! `.oxypix` files, versions 1 and 2. Little-endian, uncompressed, no external
//! schema. Version 2 is version 1 plus a list of sections after the palette;
//! a document that needs no section is still written as version 1, byte for
//! byte as before. See `docs/FORMAT.md`.
//!
//! ```text
//! magic          8 bytes  "OXYPIX1\\0"
//! version        u16
//! width, height  u32, u32
//! active_frame   u32
//! active_layer   u32
//! layer_count    u32
//! frame_count    u32
//! repeated layer_count times:
//!   name_len u16, name bytes, visible u8, opacity u8
//! repeated frame_count times:
//!   duration_ms u32
//!   repeated layer_count times: width*height*4 RGBA bytes
//! palette_count  u16
//! palette        count * 4 RGBA bytes
//! version 2 only:
//!   section_count u16
//!   repeated: kind u16, len u32, len bytes
//! ```

use std::path::Path;

use crate::blend::BlendMode;
use crate::color::{unpack, Rgba};
use crate::document::{Document, Frame, LayerInfo};
use crate::error::{Error, Result};
use crate::limits::{
    check_file_len, check_stack, clamp_name, FORMAT_VERSION, MAGIC, MAX_NAME_BYTES, MAX_TAGS,
};
use crate::tags::{FrameTag, LoopDirection};
use crate::tilemap::{Tileset, MAX_TILES, MAX_TILESETS, MAX_TILESET_PIXELS};

/// Version 1: no sections.
const VERSION_1: u16 = 1;
/// Section kinds with this bit set must be understood; a reader that does not
/// know one rejects the file. Kinds without it may be skipped.
const SECTION_REQUIRED: u16 = 0x8000;
/// Animation tags. Optional: a reader that skips it loses only the tags.
const SECTION_TAGS: u16 = 0x0001;
/// Frame names. Optional: a reader that skips it loses only the names.
const SECTION_FRAME_NAMES: u16 = 0x0002;
/// Layer blend modes. Required: a reader that drew every layer as Normal
/// would show the wrong picture, and saving from it would lose the modes.
const SECTION_BLEND_MODES: u16 = SECTION_REQUIRED | 0x0001;
/// Layer groups. Required: a reader that ignored them would flatten the
/// groups and lose them on save.
const SECTION_LAYER_GROUPS: u16 = SECTION_REQUIRED | 0x0002;
/// Tilesets. Required: their tiles are what tilemap layers draw.
const SECTION_TILESETS: u16 = SECTION_REQUIRED | 0x0003;
/// Which layers are tilemaps, and their grids in every frame. Required: a
/// reader that kept only the pixels would lose the tiles on save.
const SECTION_TILEMAPS: u16 = SECTION_REQUIRED | 0x0004;
/// A layer that is not a tilemap, in the tilemaps section.
const NO_TILESET: u16 = u16::MAX;
/// Most sections in one file.
const MAX_SECTIONS: usize = 64;

pub fn save_oxypix(doc: &Document, path: &Path) -> Result<()> {
    std::fs::write(path, save_oxypix_bytes(doc)?).map_err(Error::io)
}

pub fn load_oxypix(path: &Path) -> Result<Document> {
    let bytes = read_limited(path)?;
    load_oxypix_bytes(&bytes)
}

pub fn save_oxypix_bytes(doc: &Document) -> Result<Vec<u8>> {
    let sections = sections(doc);
    let version = if sections.is_empty() {
        VERSION_1
    } else {
        FORMAT_VERSION
    };
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&doc.width().to_le_bytes());
    out.extend_from_slice(&doc.height().to_le_bytes());
    out.extend_from_slice(&(doc.active_frame() as u32).to_le_bytes());
    out.extend_from_slice(&(doc.active_layer() as u32).to_le_bytes());
    out.extend_from_slice(&(doc.layer_count() as u32).to_le_bytes());
    out.extend_from_slice(&(doc.frame_count() as u32).to_le_bytes());
    for layer in doc.layers() {
        let name = clamp_name(&layer.name);
        let len = u16::try_from(name.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(u8::from(layer.visible));
        out.push(layer.opacity);
    }
    for frame in doc.frames() {
        out.extend_from_slice(&frame.duration_ms.to_le_bytes());
        for cel in &frame.cels {
            for px in cel.iter() {
                out.extend_from_slice(&px.to_bytes());
            }
        }
    }
    let count = u16::try_from(doc.palette().len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&count.to_le_bytes());
    for color in doc.palette() {
        out.extend_from_slice(&color.to_bytes());
    }
    if !sections.is_empty() {
        out.extend_from_slice(&(sections.len() as u16).to_le_bytes());
        for (kind, bytes) in sections {
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&bytes);
        }
    }
    Ok(out)
}

/// The version 2 sections this document needs. Empty means version 1.
fn sections(doc: &Document) -> Vec<(u16, Vec<u8>)> {
    let mut list = Vec::new();
    if !doc.tags().is_empty() {
        list.push((SECTION_TAGS, tags_bytes(doc.tags())));
    }
    if (0..doc.frame_count()).any(|i| doc.frame_name(i).is_some()) {
        list.push((SECTION_FRAME_NAMES, frame_names_bytes(doc)));
    }
    if doc.layers().iter().any(|l| l.blend != BlendMode::Normal) {
        list.push((SECTION_BLEND_MODES, blend_modes_bytes(doc)));
    }
    if doc.layers().iter().any(|l| l.group || l.depth > 0) {
        list.push((SECTION_LAYER_GROUPS, layer_groups_bytes(doc)));
    }
    if !doc.tilesets().is_empty() {
        list.push((SECTION_TILESETS, tilesets_bytes(doc)));
    }
    if doc.layers().iter().any(|l| l.tileset.is_some()) {
        list.push((SECTION_TILEMAPS, tilemaps_bytes(doc)));
    }
    list
}

/// count u16, then per tileset: name_len u16, name bytes, tile_width u16,
/// tile_height u16, tile_count u32, then every tile's RGBA pixels in order.
fn tilesets_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(doc.tilesets().len() as u16).to_le_bytes());
    for set in doc.tilesets() {
        let name = clamp_name(&set.name);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(set.tile_width() as u16).to_le_bytes());
        out.extend_from_slice(&(set.tile_height() as u16).to_le_bytes());
        out.extend_from_slice(&(set.len() as u32).to_le_bytes());
        for tile in set.tiles() {
            for px in tile {
                out.extend_from_slice(&px.to_bytes());
            }
        }
    }
    out
}

fn read_tilesets(bytes: &[u8]) -> Result<Vec<Tileset>> {
    let mut r = Reader::new(bytes);
    let count = r.u16()? as usize;
    if count > MAX_TILESETS {
        return Err(Error::Limit {
            what: "tilesets",
            value: count as u64,
            max: MAX_TILESETS as u64,
        });
    }
    let mut sets = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = r.u16()? as usize;
        if name_len > MAX_NAME_BYTES {
            return Err(Error::BadName);
        }
        let name = std::str::from_utf8(r.bytes(name_len)?)
            .map_err(|_| Error::BadName)?
            .to_string();
        let tile_width = u32::from(r.u16()?);
        let tile_height = u32::from(r.u16()?);
        let mut set = Tileset::new(name, tile_width, tile_height)?;
        let tiles = r.u32()? as u64;
        let total = tiles * u64::from(tile_width) * u64::from(tile_height);
        if tiles > MAX_TILES as u64 || total > MAX_TILESET_PIXELS {
            return Err(Error::Limit {
                what: "pixels in a tileset",
                value: total,
                max: MAX_TILESET_PIXELS,
            });
        }
        let tile_bytes = (tile_width * tile_height * 4) as usize;
        for _ in 0..tiles {
            let pixels = unpack(r.bytes(tile_bytes)?).ok_or(Error::Truncated)?;
            set.push(pixels)?;
        }
        sets.push(set);
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Ok(sets)
}

/// count u16 (the layer count), then per layer its tileset u16 (65535 for
/// a layer that is not a tilemap), then for each frame, for each tilemap
/// layer bottom first, its grid: cols*rows u32 entries, row by row.
fn tilemaps_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(doc.layer_count() as u16).to_le_bytes());
    for layer in doc.layers() {
        out.extend_from_slice(&layer.tileset.unwrap_or(NO_TILESET).to_le_bytes());
    }
    for frame in 0..doc.frame_count() {
        for layer in 0..doc.layer_count() {
            if let Some(grid) = doc.tile_grid(frame, layer) {
                for value in grid {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
    }
    out
}

/// Each layer's tileset, and `grids[frame][layer]`. Grid sizes come from
/// the tilesets.
#[allow(clippy::type_complexity)]
fn read_tilemaps(
    bytes: &[u8],
    tilesets: &[Tileset],
    (width, height): (u32, u32),
    layer_count: usize,
    frame_count: usize,
) -> Result<(Vec<Option<u16>>, Vec<Vec<Option<Vec<u32>>>>)> {
    let mut r = Reader::new(bytes);
    if r.u16()? as usize != layer_count {
        return Err(Error::BadFlag("tilemap layer count"));
    }
    let mut layer_sets = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        layer_sets.push(match r.u16()? {
            NO_TILESET => None,
            set if usize::from(set) < tilesets.len() => Some(set),
            _ => return Err(Error::BadFlag("tilemap tileset")),
        });
    }
    let mut grids = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let mut frame = Vec::with_capacity(layer_count);
        for set in &layer_sets {
            frame.push(match set {
                None => None,
                Some(set) => {
                    let (cols, rows) = tilesets[usize::from(*set)].grid_size(width, height);
                    let cells = cols as usize * rows as usize;
                    let raw = r.bytes(cells * 4)?;
                    Some(
                        raw.chunks_exact(4)
                            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect(),
                    )
                }
            });
        }
        grids.push(frame);
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Ok((layer_sets, grids))
}

/// count u16 (the layer count), then per layer, bottom first: flags u8
/// (bit 0: a group) and depth u8 (how many groups it is inside).
fn layer_groups_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(doc.layer_count() as u16).to_le_bytes());
    for layer in doc.layers() {
        out.push(u8::from(layer.group));
        out.push(layer.depth);
    }
    out
}

fn read_layer_groups(bytes: &[u8], layer_count: usize) -> Result<Vec<(bool, u8)>> {
    let mut r = Reader::new(bytes);
    if r.u16()? as usize != layer_count {
        return Err(Error::BadFlag("layer group count"));
    }
    let mut out = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let group = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(Error::BadFlag("layer group flags")),
        };
        out.push((group, r.u8()?));
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Ok(out)
}

/// count u16 (the layer count), then one u8 mode code per layer, bottom first.
fn blend_modes_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(doc.layer_count() as u16).to_le_bytes());
    out.extend(doc.layers().iter().map(|l| l.blend.code() as u8));
    out
}

fn read_blend_modes(bytes: &[u8], layer_count: usize) -> Result<Vec<BlendMode>> {
    let mut r = Reader::new(bytes);
    if r.u16()? as usize != layer_count {
        return Err(Error::BadFlag("blend mode count"));
    }
    let mut modes = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let mode =
            BlendMode::from_code(u16::from(r.u8()?)).ok_or(Error::BadFlag("layer blend mode"))?;
        modes.push(mode);
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Ok(modes)
}

/// count u16 (the frame count), then per frame: name_len u16, name bytes.
/// An unnamed frame has length 0.
fn frame_names_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(doc.frame_count() as u16).to_le_bytes());
    for index in 0..doc.frame_count() {
        let name = clamp_name(doc.frame_name(index).unwrap_or(""));
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    out
}

fn read_frame_names(bytes: &[u8], frame_count: usize) -> Result<Vec<String>> {
    let mut r = Reader::new(bytes);
    if r.u16()? as usize != frame_count {
        return Err(Error::BadFlag("frame name count"));
    }
    let mut names = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let len = r.u16()? as usize;
        if len > MAX_NAME_BYTES {
            return Err(Error::BadName);
        }
        let name = std::str::from_utf8(r.bytes(len)?).map_err(|_| Error::BadName)?;
        names.push(name.to_string());
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Ok(names)
}

/// count u16, then per tag: from u16, to u16, direction u8, repeat u16,
/// name_len u16, name bytes.
fn tags_bytes(tags: &[FrameTag]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    for tag in tags {
        let name = clamp_name(&tag.name);
        out.extend_from_slice(&(tag.from as u16).to_le_bytes());
        out.extend_from_slice(&(tag.to as u16).to_le_bytes());
        out.push(tag.direction.code());
        out.extend_from_slice(&tag.repeat.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    out
}

fn read_tags(bytes: &[u8]) -> Result<Vec<FrameTag>> {
    let mut r = Reader::new(bytes);
    let count = r.u16()? as usize;
    if count > MAX_TAGS {
        return Err(Error::Limit {
            what: "tags",
            value: count as u64,
            max: MAX_TAGS as u64,
        });
    }
    let mut tags = Vec::with_capacity(count);
    for _ in 0..count {
        let from = r.u16()? as usize;
        let to = r.u16()? as usize;
        let direction = LoopDirection::from_code(r.u8()?).ok_or(Error::BadFlag("tag direction"))?;
        let repeat = r.u16()?;
        let name_len = r.u16()? as usize;
        if name_len > MAX_NAME_BYTES {
            return Err(Error::BadName);
        }
        let name = std::str::from_utf8(r.bytes(name_len)?)
            .map_err(|_| Error::BadName)?
            .to_string();
        tags.push(FrameTag {
            name,
            from,
            to,
            direction,
            repeat,
        });
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Ok(tags)
}

pub fn load_oxypix_bytes(bytes: &[u8]) -> Result<Document> {
    check_file_len(bytes.len() as u64)?;
    let mut r = Reader::new(bytes);
    let magic = r.bytes(8)?;
    if magic != MAGIC {
        return Err(Error::BadMagic);
    }
    let version = r.u16()?;
    if version != VERSION_1 && version != FORMAT_VERSION {
        return Err(Error::BadVersion(version));
    }
    let width = r.u32()?;
    let height = r.u32()?;
    let active_frame = r.u32()? as usize;
    let active_layer = r.u32()? as usize;
    let layer_count = r.u32()? as usize;
    let frame_count = r.u32()? as usize;
    // Reject hostile counts before any pixel allocation.
    check_stack(width, height, frame_count, layer_count)?;

    let mut layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        let name_len = r.u16()? as usize;
        if name_len > MAX_NAME_BYTES {
            return Err(Error::BadName);
        }
        let name_bytes = r.bytes(name_len)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| Error::BadName)?
            .to_string();
        let visible = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(Error::BadFlag("layer visibility")),
        };
        let opacity = r.u8()?;
        layers.push(LayerInfo {
            visible,
            opacity,
            ..LayerInfo::new(name)
        });
    }

    let cel_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::Truncated)?;
    let mut frames = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let duration_ms = r.u32()?;
        let mut cels = Vec::with_capacity(layer_count);
        for _ in 0..layer_count {
            let raw = r.bytes(cel_bytes)?;
            let pixels = unpack(raw).ok_or(Error::Truncated)?;
            cels.push(pixels.into());
        }
        frames.push(Frame {
            duration_ms,
            name: String::new(),
            cels,
        });
    }

    let palette_count = r.u16()? as usize;
    if palette_count == 0 || palette_count > crate::limits::MAX_PALETTE {
        return Err(Error::EmptyPalette);
    }
    let mut palette = Vec::with_capacity(palette_count);
    for _ in 0..palette_count {
        let raw = r.bytes(4)?;
        palette.push(Rgba::new(raw[0], raw[1], raw[2], raw[3]));
    }
    let mut tags = None;
    let mut frame_names = None;
    let mut blend_modes = None;
    let mut groups = None;
    let mut tilesets = None;
    let mut tilemaps = None;
    if version != VERSION_1 {
        let count = r.u16()? as usize;
        if count > MAX_SECTIONS {
            return Err(Error::Limit {
                what: "file sections",
                value: count as u64,
                max: MAX_SECTIONS as u64,
            });
        }
        for _ in 0..count {
            let kind = r.u16()?;
            let len = r.u32()? as usize;
            let body = r.bytes(len)?;
            match kind {
                SECTION_TAGS if tags.is_some() => return Err(Error::DuplicateSection("tags")),
                SECTION_TAGS => tags = Some(read_tags(body)?),
                SECTION_FRAME_NAMES if frame_names.is_some() => {
                    return Err(Error::DuplicateSection("frame names"))
                }
                SECTION_FRAME_NAMES => frame_names = Some(read_frame_names(body, frame_count)?),
                SECTION_BLEND_MODES if blend_modes.is_some() => {
                    return Err(Error::DuplicateSection("blend modes"))
                }
                SECTION_BLEND_MODES => blend_modes = Some(read_blend_modes(body, layers.len())?),
                SECTION_LAYER_GROUPS if groups.is_some() => {
                    return Err(Error::DuplicateSection("layer groups"))
                }
                SECTION_LAYER_GROUPS => groups = Some(read_layer_groups(body, layers.len())?),
                SECTION_TILESETS if tilesets.is_some() => {
                    return Err(Error::DuplicateSection("tilesets"))
                }
                SECTION_TILESETS => tilesets = Some(read_tilesets(body)?),
                // Read once the tilesets are known: the grid sizes come from them.
                SECTION_TILEMAPS if tilemaps.is_some() => {
                    return Err(Error::DuplicateSection("tilemaps"))
                }
                SECTION_TILEMAPS => tilemaps = Some(body),
                kind if kind & SECTION_REQUIRED != 0 => return Err(Error::RequiredSection(kind)),
                // An optional section from a newer writer: skip it.
                _ => {}
            }
        }
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    for (layer, mode) in layers.iter_mut().zip(blend_modes.into_iter().flatten()) {
        layer.blend = mode;
    }
    for (layer, (group, depth)) in layers.iter_mut().zip(groups.into_iter().flatten()) {
        layer.group = group;
        layer.depth = depth;
    }
    let mut doc = Document::from_parts(
        width,
        height,
        layers,
        frames,
        active_frame,
        active_layer,
        palette,
    )?;
    if let Some(tags) = tags {
        doc.set_tags(tags)?;
    }
    for (index, name) in frame_names.into_iter().flatten().enumerate() {
        doc.set_frame_name(index, &name)?;
    }
    let tilesets = tilesets.unwrap_or_default();
    if let Some(body) = tilemaps {
        let (layer_sets, grids) = read_tilemaps(
            body,
            &tilesets,
            (width, height),
            doc.layer_count(),
            doc.frame_count(),
        )?;
        doc.attach_tilemaps(tilesets, layer_sets, grids)?;
    } else if !tilesets.is_empty() {
        let layer_sets = vec![None; doc.layer_count()];
        let grids = vec![vec![None; doc.layer_count()]; doc.frame_count()];
        doc.attach_tilemaps(tilesets, layer_sets, grids)?;
    }
    Ok(doc)
}

pub(crate) fn read_limited(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(Error::io)?;
    check_file_len(meta.len())?;
    let bytes = std::fs::read(path).map_err(Error::io)?;
    check_file_len(bytes.len() as u64)?;
    Ok(bytes)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn finished(&self) -> bool {
        self.pos == self.data.len()
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        if end > self.data.len() {
            return Err(Error::Truncated);
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::limits::{MAGIC, MAX_FILE_BYTES};

    fn tiled() -> Document {
        let mut doc = Document::new(3, 3).unwrap();
        let set = doc.add_tileset("ground", 2, 2).unwrap();
        let red = Rgba::new(255, 0, 0, 255);
        doc.add_tile(set, vec![red, Rgba::CLEAR, Rgba::CLEAR, red])
            .unwrap();
        doc.add_tilemap_layer("map", set).unwrap();
        doc.set_tile(0, 1, 1, 1, 1 | crate::tilemap::TILE_FLIP_X)
            .unwrap();
        doc.add_frame().unwrap();
        doc.set_tile(1, 1, 0, 0, 1).unwrap();
        doc
    }

    #[test]
    fn tilesets_and_grids_round_trip_in_required_sections() {
        let doc = tiled();
        let bytes = save_oxypix_bytes(&doc).unwrap();
        let back = load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(back, doc);
        assert_eq!(
            back.tile_grid(0, 1).unwrap(),
            &[0, 0, 0, 1 | crate::tilemap::TILE_FLIP_X]
        );
        // The mirrored tile at (2, 2) is clipped to one pixel: the tile's top-right.
        assert_eq!(back.cel(0, 1).unwrap()[8], Rgba::CLEAR);
        assert_eq!(back.tilesets()[0].name, "ground");
        let kinds = section_kinds(&bytes);
        assert!(kinds.contains(&SECTION_TILESETS) && kinds.contains(&SECTION_TILEMAPS));
        // Unused tilesets are kept without the tilemaps section.
        let mut unused = Document::new(2, 2).unwrap();
        unused.add_tileset("spare", 4, 4).unwrap();
        let back = load_oxypix_bytes(&save_oxypix_bytes(&unused).unwrap()).unwrap();
        assert_eq!(back.tilesets().len(), 1);
    }

    #[test]
    fn broken_tilemap_sections_are_errors() {
        let doc = tiled();
        let sections = sections(&doc);
        let tilemaps = sections
            .iter()
            .find(|(k, _)| *k == SECTION_TILEMAPS)
            .unwrap()
            .1
            .clone();
        let tilesets = read_tilesets(
            &sections
                .iter()
                .find(|(k, _)| *k == SECTION_TILESETS)
                .unwrap()
                .1,
        )
        .unwrap();
        let read = |body: &[u8]| read_tilemaps(body, &tilesets, (3, 3), 2, 2);
        assert!(read(&tilemaps).is_ok());
        let mut extra = tilemaps.clone();
        extra.push(0);
        assert!(matches!(read(&extra), Err(Error::TrailingBytes)));
        assert!(read(&tilemaps[..tilemaps.len() - 1]).is_err());
        let mut missing_set = tilemaps.clone();
        missing_set[4] = 7; // layer 1's tileset
        assert!(read(&missing_set).is_err());
        // A tile number past the end of the tileset.
        let (sets, mut grids) = read(&tilemaps).unwrap();
        grids[0][1].as_mut().unwrap()[0] = 9;
        let mut plain = Document::new(3, 3).unwrap();
        plain.add_layer("map").unwrap();
        plain.add_frame().unwrap();
        assert!(plain
            .attach_tilemaps(tilesets.clone(), sets.clone(), grids)
            .is_err());
        // A tileset whose pixels run short.
        let mut set_bytes = sections
            .iter()
            .find(|(k, _)| *k == SECTION_TILESETS)
            .unwrap()
            .1
            .clone();
        set_bytes.pop();
        assert!(read_tilesets(&set_bytes).is_err());
    }

    fn section_kinds(bytes: &[u8]) -> Vec<u16> {
        let doc = load_oxypix_bytes(bytes).unwrap();
        sections(&doc).into_iter().map(|(kind, _)| kind).collect()
    }

    #[test]
    fn roundtrip_preserves_pixels_layers_and_palette() {
        let mut doc = Document::new(3, 2).unwrap();
        doc.add_layer("Ink").unwrap();
        doc.frames_mut()[0].cels[1][4] = Rgba::new(1, 2, 3, 4);
        doc.layers_mut()[0].visible = false;
        doc.layers_mut()[1].opacity = 90;
        doc.add_frame().unwrap();
        doc.set_frame_duration(1, 240).unwrap();
        doc.set_active_frame(1).unwrap();
        doc.palette_mut().push(Rgba::new(9, 9, 9, 255));
        let bytes = save_oxypix_bytes(&doc).unwrap();
        let loaded = load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(loaded, doc);
    }

    #[test]
    fn corrupt_inputs_fail_cleanly() {
        assert!(matches!(load_oxypix_bytes(b""), Err(Error::Truncated)));
        assert!(matches!(
            load_oxypix_bytes(b"not oxyp"),
            Err(Error::BadMagic)
        ));
        let mut bad_ver = MAGIC.to_vec();
        bad_ver.extend_from_slice(&99u16.to_le_bytes());
        assert!(matches!(
            load_oxypix_bytes(&bad_ver),
            Err(Error::BadVersion(99))
        ));

        let mut hostile = MAGIC.to_vec();
        hostile.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        hostile.extend_from_slice(&64u32.to_le_bytes());
        hostile.extend_from_slice(&64u32.to_le_bytes());
        hostile.extend_from_slice(&0u32.to_le_bytes());
        hostile.extend_from_slice(&0u32.to_le_bytes());
        hostile.extend_from_slice(&1_000_000u32.to_le_bytes());
        hostile.extend_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            load_oxypix_bytes(&hostile),
            Err(Error::Limit { .. })
        ));

        let mut doc = save_oxypix_bytes(&Document::new(1, 1).unwrap()).unwrap();
        doc.push(0);
        assert!(matches!(load_oxypix_bytes(&doc), Err(Error::TrailingBytes)));
    }

    fn tagged(frames: usize) -> Document {
        let mut doc = Document::new(2, 1).unwrap();
        for _ in 1..frames {
            doc.add_frame().unwrap();
        }
        doc
    }

    fn tag(name: &str, from: usize, to: usize, direction: LoopDirection, repeat: u16) -> FrameTag {
        FrameTag {
            name: name.into(),
            from,
            to,
            direction,
            repeat,
        }
    }

    /// Version 1 bytes of `doc` turned into version 2 with these sections.
    fn with_sections(doc: &Document, sections: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut bytes = save_oxypix_bytes(doc).unwrap();
        bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(sections.len() as u16).to_le_bytes());
        for (kind, body) in sections {
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
            bytes.extend_from_slice(body);
        }
        bytes
    }

    #[test]
    fn layer_groups_are_a_required_section_and_round_trip() {
        let mut doc = Document::demo().unwrap();
        doc.add_group("Characters").unwrap();
        doc.add_layer("Hero").unwrap();
        doc.indent_layer(3).unwrap();
        let bytes = save_oxypix_bytes(&doc).unwrap();
        assert_eq!(&bytes[8..10], &2u16.to_le_bytes());
        let back = load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(back.layers(), doc.layers());
        assert_ne!(SECTION_LAYER_GROUPS & SECTION_REQUIRED, 0);

        let plain = Document::demo().unwrap();
        let section = |body: Vec<u8>| with_sections(&plain, &[(SECTION_LAYER_GROUPS, body)]);
        let body = |pairs: &[u8]| {
            let mut out = 2u16.to_le_bytes().to_vec();
            out.extend_from_slice(pairs);
            out
        };
        // A group on top holding nothing, over a plain layer: fine.
        assert!(load_oxypix_bytes(&section(body(&[0, 0, 1, 0]))).is_ok());
        // Mark is one level deep but Paper is not a group.
        assert!(load_oxypix_bytes(&section(body(&[0, 0, 0, 1]))).is_err());
        // Every layer a group: nothing to draw on.
        assert!(load_oxypix_bytes(&section(body(&[1, 0, 1, 0]))).is_err());
        assert!(
            load_oxypix_bytes(&section(body(&[0, 0, 2, 0]))).is_err(),
            "bad flag"
        );
        assert!(load_oxypix_bytes(&section(body(&[0, 0]))).is_err(), "short");
        let mut long = body(&[0, 0, 0, 0]);
        long.push(0);
        assert!(load_oxypix_bytes(&section(long)).is_err());
        let twice = with_sections(
            &plain,
            &[
                (SECTION_LAYER_GROUPS, body(&[0, 0, 0, 0])),
                (SECTION_LAYER_GROUPS, body(&[0, 0, 0, 0])),
            ],
        );
        assert!(load_oxypix_bytes(&twice).is_err());
    }

    #[test]
    fn blend_modes_are_a_required_section_and_round_trip() {
        let mut doc = Document::demo().unwrap();
        doc.layers_mut()[1].blend = BlendMode::Multiply;
        let bytes = save_oxypix_bytes(&doc).unwrap();
        assert_eq!(&bytes[8..10], &2u16.to_le_bytes());
        let back = load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(back.layers()[0].blend, BlendMode::Normal);
        assert_eq!(back.layers()[1].blend, BlendMode::Multiply);
        assert_eq!(back.composite(0).unwrap(), doc.composite(0).unwrap());
        assert_ne!(SECTION_BLEND_MODES & SECTION_REQUIRED, 0);

        let mut plain = doc.clone();
        plain.layers_mut()[1].blend = BlendMode::Normal;
        assert_eq!(
            &save_oxypix_bytes(&plain).unwrap()[8..10],
            &1u16.to_le_bytes()
        );

        let mut wrong_count = 1u16.to_le_bytes().to_vec();
        wrong_count.push(1);
        let err = load_oxypix_bytes(&with_sections(
            &plain,
            &[(SECTION_BLEND_MODES, wrong_count)],
        ));
        assert!(err.is_err());
        let mut bad_code = 2u16.to_le_bytes().to_vec();
        bad_code.extend([0, 99]);
        assert!(
            load_oxypix_bytes(&with_sections(&plain, &[(SECTION_BLEND_MODES, bad_code)])).is_err()
        );
        let mut good = 2u16.to_le_bytes().to_vec();
        good.extend([0, 2]);
        let twice = with_sections(
            &plain,
            &[
                (SECTION_BLEND_MODES, good.clone()),
                (SECTION_BLEND_MODES, good.clone()),
            ],
        );
        assert!(load_oxypix_bytes(&twice).is_err());
        let mut long = good.clone();
        long.push(0);
        assert!(load_oxypix_bytes(&with_sections(&plain, &[(SECTION_BLEND_MODES, long)])).is_err());
        let screen =
            load_oxypix_bytes(&with_sections(&plain, &[(SECTION_BLEND_MODES, good)])).unwrap();
        assert_eq!(screen.layers()[1].blend, BlendMode::Screen);
    }

    #[test]
    fn a_document_without_tags_is_still_version_1() {
        let doc = Document::demo().unwrap();
        let bytes = save_oxypix_bytes(&doc).unwrap();
        assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
        // Header, two layers named "Paper" and "Mark", two 32x32 frames, the palette.
        let layers = (2 + 5 + 2) + (2 + 4 + 2);
        let frames = 2 * (4 + 2 * 32 * 32 * 4);
        let palette = 2 + doc.palette().len() * 4;
        assert_eq!(bytes.len(), 8 + 2 + 6 * 4 + layers + frames + palette);
        assert_eq!(load_oxypix_bytes(&bytes).unwrap(), doc);
    }

    #[test]
    fn tags_round_trip_in_version_2() {
        let mut doc = tagged(4);
        doc.add_tag(tag("walk", 0, 2, LoopDirection::PingPong, 3))
            .unwrap();
        doc.add_tag(tag("idle ✨", 3, 3, LoopDirection::Reverse, 0))
            .unwrap();
        let bytes = save_oxypix_bytes(&doc).unwrap();
        assert_eq!(&bytes[8..10], &2u16.to_le_bytes());
        let back = load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(back, doc);
        assert_eq!(back.tags()[1].name, "idle ✨");
        // Removing the last tag goes back to version 1.
        doc.remove_tag(1).unwrap();
        doc.remove_tag(0).unwrap();
        assert_eq!(
            &save_oxypix_bytes(&doc).unwrap()[8..10],
            &1u16.to_le_bytes()
        );
    }

    #[test]
    fn version_2_sections_are_checked() {
        let doc = tagged(2);
        let good_tags = tags_bytes(&[tag("a", 0, 1, LoopDirection::Forward, 0)]);
        // An unknown optional section is skipped; an unknown required one is refused.
        let skipped = with_sections(
            &doc,
            &[(0x0042, vec![1, 2, 3]), (SECTION_TAGS, good_tags.clone())],
        );
        assert_eq!(load_oxypix_bytes(&skipped).unwrap().tags().len(), 1);
        let required = with_sections(&doc, &[(0x8042, vec![])]);
        assert!(matches!(
            load_oxypix_bytes(&required),
            Err(Error::RequiredSection(0x8042))
        ));
        let twice = with_sections(
            &doc,
            &[
                (SECTION_TAGS, good_tags.clone()),
                (SECTION_TAGS, good_tags.clone()),
            ],
        );
        assert!(matches!(
            load_oxypix_bytes(&twice),
            Err(Error::DuplicateSection("tags"))
        ));
        // A tag past the last frame, a bad direction, and extra bytes in the section.
        let far = with_sections(
            &doc,
            &[(
                SECTION_TAGS,
                tags_bytes(&[tag("a", 0, 2, LoopDirection::Forward, 0)]),
            )],
        );
        assert!(matches!(load_oxypix_bytes(&far), Err(Error::BadTag(_))));
        let mut direction = good_tags.clone();
        direction[2 + 4] = 9;
        let bad_direction = with_sections(&doc, &[(SECTION_TAGS, direction)]);
        assert!(matches!(
            load_oxypix_bytes(&bad_direction),
            Err(Error::BadFlag("tag direction"))
        ));
        let mut padded = good_tags.clone();
        padded.push(0);
        let long = with_sections(&doc, &[(SECTION_TAGS, padded)]);
        assert!(matches!(
            load_oxypix_bytes(&long),
            Err(Error::TrailingBytes)
        ));
        // Version 2 with no section list at all is truncated, and every cut of a good file fails.
        let good = with_sections(&doc, &[(SECTION_TAGS, good_tags)]);
        let header_only = &good[..good.len() - (2 + 2 + 4 + 2 + 2 + 2 + 1 + 2 + 2 + 1)];
        assert!(load_oxypix_bytes(header_only).is_err());
        for len in 0..good.len() {
            assert!(load_oxypix_bytes(&good[..len]).is_err(), "cut at {len}");
        }
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(matches!(
            load_oxypix_bytes(&trailing),
            Err(Error::TrailingBytes)
        ));
    }

    #[test]
    fn frame_names_round_trip_and_are_checked() {
        let mut doc = tagged(3);
        doc.set_frame_name(0, "idle").unwrap();
        doc.set_frame_name(2, "jump ✨").unwrap();
        assert_eq!(doc.frame_name(0), Some("idle"));
        assert_eq!(doc.frame_name(1), None);
        let bytes = save_oxypix_bytes(&doc).unwrap();
        assert_eq!(&bytes[8..10], &2u16.to_le_bytes());
        let back = load_oxypix_bytes(&bytes).unwrap();
        assert_eq!(back, doc);
        // Clearing every name goes back to version 1.
        doc.set_frame_name(0, "").unwrap();
        doc.set_frame_name(2, " ").unwrap();
        assert_eq!(
            &save_oxypix_bytes(&doc).unwrap()[8..10],
            &1u16.to_le_bytes()
        );

        let plain = tagged(2);
        let mut wrong_count = Vec::new();
        wrong_count.extend_from_slice(&3u16.to_le_bytes());
        for _ in 0..3 {
            wrong_count.extend_from_slice(&0u16.to_le_bytes());
        }
        let bad = with_sections(&plain, &[(SECTION_FRAME_NAMES, wrong_count)]);
        assert!(matches!(
            load_oxypix_bytes(&bad),
            Err(Error::BadFlag("frame name count"))
        ));
        let mut named = plain.clone();
        named.set_frame_name(1, "b").unwrap();
        let body = frame_names_bytes(&named);
        let twice = with_sections(
            &plain,
            &[
                (SECTION_FRAME_NAMES, body.clone()),
                (SECTION_FRAME_NAMES, body.clone()),
            ],
        );
        assert!(matches!(
            load_oxypix_bytes(&twice),
            Err(Error::DuplicateSection("frame names"))
        ));
        let mut not_utf8 = body;
        let last = not_utf8.len() - 1;
        not_utf8[last] = 0xff;
        let bad_text = with_sections(&plain, &[(SECTION_FRAME_NAMES, not_utf8)]);
        assert!(matches!(load_oxypix_bytes(&bad_text), Err(Error::BadName)));
    }

    #[test]
    fn file_size_cap_is_enforced_without_reading_pixels() {
        let err = check_file_len(MAX_FILE_BYTES + 1).unwrap_err();
        assert!(matches!(err, Error::Limit { .. }));
    }
}
