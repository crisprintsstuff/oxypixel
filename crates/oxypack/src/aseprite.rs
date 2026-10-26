//! Read Aseprite `.aseprite` / `.ase` files.
//!
//! Written from Aseprite's published file format description
//! (`docs/ase-file-specs.md` in the Aseprite repository). No Aseprite code is
//! used. Each frame is composited the way the packer needs it: visible
//! layers from the bottom up, with layer and cel opacity. Every size is
//! checked before memory is allocated for it.
//!
//! Layers blend with their mode (see `blend.rs`). When the file says group
//! opacity and blend mode are valid, a group is drawn as one image and then
//! blended with its own opacity and mode, as Aseprite does; otherwise its
//! children draw straight through. A background layer is always opaque.
//! Reference layers are skipped, as Aseprite's own export does.
//!
//! A tilemap cel is drawn from its layer's tileset like an image cel. Tile 0
//! is empty, and a tile number past the end of the tileset is drawn empty
//! with a note. Flipped tiles follow the usual tilemap convention: the
//! diagonal flip (swap x and y, square tiles only) first, then the x flip,
//! then the y flip. Tilesets kept in another file and unknown blend modes
//! are not supported; they get a note instead of an error.

use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;

use oxypixel_core::{Rgba, DEFAULT_FRAME_MS, MAX_FILE_BYTES, MAX_SHEET_PIXELS};

use crate::blend::{blend, BlendMode};

/// Largest canvas side accepted. The atlas cannot be wider than this anyway.
const MAX_SIDE: u32 = oxypixel_core::MAX_SHEET_WIDTH;
/// Most frames in one file.
const MAX_ASE_FRAMES: usize = 4096;
/// Most pixels kept for cels at once (linked cels point back at earlier ones).
const MAX_CEL_STORE: u64 = MAX_SHEET_PIXELS;

const HEADER_MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;
const HEADER_LEN: usize = 128;
const FRAME_HEADER_LEN: usize = 16;

const CHUNK_OLD_PALETTE: u16 = 0x0004;
const CHUNK_OLD_PALETTE_64: u16 = 0x0011;
const CHUNK_LAYER: u16 = 0x2004;
const CHUNK_CEL: u16 = 0x2005;
const CHUNK_TAGS: u16 = 0x2018;
const CHUNK_PALETTE: u16 = 0x2019;
const CHUNK_TILESET: u16 = 0x2023;

const TILESET_EXTERNAL: u32 = 1;
const TILESET_INSIDE: u32 = 2;
const TILESET_ZERO_EMPTY: u32 = 4;

const FLAG_LAYER_OPACITY: u32 = 1;
const FLAG_GROUP_OPACITY: u32 = 2;
const FLAG_LAYER_UUID: u32 = 4;

const LAYER_VISIBLE: u16 = 1;
const LAYER_BACKGROUND: u16 = 8;
const LAYER_REFERENCE: u16 = 64;

/// A named frame range, counted from 0 in this file. The same type the
/// editor stores in `.oxypix` version 2.
pub use oxypixel_core::FrameTag as Tag;
/// How a tag plays its frames.
pub use oxypixel_core::LoopDirection as Direction;

/// A decoded file: one composited image per frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AseFile {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha pixels and the duration in milliseconds, per frame.
    pub frames: Vec<(Vec<Rgba>, u32)>,
    pub tags: Vec<Tag>,
    /// Things that were drawn differently from Aseprite, one line each.
    pub notes: Vec<String>,
}

/// Why a file could not be read. The text says what was wrong.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct AseError(pub String);

fn bad(text: impl Into<String>) -> AseError {
    AseError(text.into())
}

type Result<T> = std::result::Result<T, AseError>;

/// A bounds-checked little-endian reader.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| bad("the file is truncated"))?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    fn string(&mut self) -> Result<String> {
        let len = usize::from(self.u16()?);
        let bytes = self.take(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn rest(&mut self) -> &'a [u8] {
        let bytes = &self.data[self.pos..];
        self.pos = self.data.len();
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Depth {
    Rgba,
    Grayscale,
    Indexed,
}

impl Depth {
    fn bytes(self) -> usize {
        match self {
            Self::Rgba => 4,
            Self::Grayscale => 2,
            Self::Indexed => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Image,
    Group,
    Tilemap,
}

struct Layer {
    name: String,
    /// The group this layer is in, from the child levels.
    parent: Option<usize>,
    kind: Kind,
    /// This layer's own visibility. A hidden group hides its children.
    visible: bool,
    /// This layer's own opacity: 255 when the header says opacity is not
    /// valid, and always 255 on a background layer.
    opacity: u8,
    blend: BlendMode,
    background: bool,
    /// Reference layers are not drawn, as in Aseprite's export.
    reference: bool,
    /// The tileset a tilemap layer draws from.
    tileset: u32,
}

/// Tiles stacked top to bottom in one image, `tile_w` wide.
struct Tileset {
    tile_w: u32,
    tile_h: u32,
    count: u32,
    /// Tile 0 is the empty tile. Without this flag the empty tile is the
    /// tile number with every bit of the mask set.
    zero_empty: bool,
    /// `None` when the tiles are kept in another file.
    pixels: Option<Vec<Rgba>>,
}

/// A tilemap cel, drawn into an image once the frame's chunks are read.
struct Tilemap {
    layer: usize,
    x: i32,
    y: i32,
    opacity: u8,
    z: i16,
    cols: u32,
    rows: u32,
    tiles: Vec<u32>,
    id_mask: u32,
    x_flip: u32,
    y_flip: u32,
    d_flip: u32,
}

/// Cel pixels. Linked cels share one.
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
}

struct Cel {
    layer: usize,
    x: i32,
    y: i32,
    opacity: u8,
    z: i16,
    image: Rc<Image>,
}

fn mul255(a: u8, b: u8) -> u8 {
    ((u32::from(a) * u32::from(b) + 127) / 255) as u8
}

/// Decode a whole file. Bytes past the declared size are ignored.
pub fn read_aseprite(bytes: &[u8]) -> Result<AseFile> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(bad(format!(
            "the file is larger than {} MiB",
            MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    let mut head = Cursor::new(bytes);
    let declared = head.u32()? as usize;
    if head.u16()? != HEADER_MAGIC {
        return Err(bad("this is not an Aseprite file"));
    }
    let frame_count = usize::from(head.u16()?);
    let width = u32::from(head.u16()?);
    let height = u32::from(head.u16()?);
    let depth = match head.u16()? {
        32 => Depth::Rgba,
        16 => Depth::Grayscale,
        8 => Depth::Indexed,
        other => return Err(bad(format!("unsupported color depth {other}"))),
    };
    let flags = head.u32()?;
    head.skip(2 + 4 + 4)?; // speed (deprecated), two zero DWORDs
    let transparent_index = head.u8()?;
    head.skip(3)?;
    let color_count = head.u16()?;
    head.skip(HEADER_LEN - head.pos)?;

    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(bad(format!(
            "canvas {width}×{height} is outside 1×1 … {MAX_SIDE}×{MAX_SIDE}"
        )));
    }
    if frame_count == 0 || frame_count > MAX_ASE_FRAMES {
        return Err(bad(format!(
            "{frame_count} frames is outside 1 … {MAX_ASE_FRAMES}"
        )));
    }
    let frame_pixels = u64::from(width) * u64::from(height);
    if frame_pixels * frame_count as u64 > MAX_SHEET_PIXELS {
        return Err(bad(
            "the frames together are larger than one atlas can hold",
        ));
    }
    let end = declared.clamp(HEADER_LEN, bytes.len());

    let layer_opacity_valid = flags & FLAG_LAYER_OPACITY != 0;
    let group_opacity_valid = flags & FLAG_GROUP_OPACITY != 0;
    let layer_uuid = flags & FLAG_LAYER_UUID != 0;
    let mut palette = vec![Rgba::CLEAR; 256.max(usize::from(color_count))];
    let mut layers: Vec<Layer> = Vec::new();
    // Each group's visibility and opacity, by child level.
    // The open groups, by child level.
    let mut groups: Vec<usize> = Vec::new();
    let mut tags = Vec::new();
    let mut notes = Vec::new();
    let mut tilesets: HashMap<u32, Tileset> = HashMap::new();
    // Each layer's cel image, by frame, for linked cels.
    let mut store: Vec<Vec<Option<Rc<Image>>>> = Vec::new();
    let mut stored_pixels = 0u64;
    let mut frames = Vec::with_capacity(frame_count);

    let mut pos = HEADER_LEN;
    for frame in 0..frame_count {
        let mut fh = Cursor::new(&bytes[..end]);
        fh.pos = pos;
        let frame_len = fh.u32()? as usize;
        if fh.u16()? != FRAME_MAGIC {
            return Err(bad(format!("frame {frame} has a bad header")));
        }
        fh.skip(2)?; // old chunk count; chunks are read up to the frame's end
        let duration = match u32::from(fh.u16()?) {
            0 => DEFAULT_FRAME_MS,
            ms => ms,
        };
        fh.skip(2 + 4)?;
        let frame_end = pos
            .checked_add(frame_len)
            .filter(|&e| frame_len >= FRAME_HEADER_LEN && e <= end)
            .ok_or_else(|| bad(format!("frame {frame} runs past the end of the file")))?;
        let mut cels: Vec<Cel> = Vec::new();
        let mut tilemaps: Vec<Tilemap> = Vec::new();
        let mut at = pos + FRAME_HEADER_LEN;
        while at < frame_end {
            let mut ch = Cursor::new(&bytes[..frame_end]);
            ch.pos = at;
            let size = ch.u32()? as usize;
            let kind = ch.u16()?;
            let chunk_end = at
                .checked_add(size)
                .filter(|&e| size >= 6 && e <= frame_end)
                .ok_or_else(|| bad(format!("a chunk in frame {frame} runs past its frame")))?;
            let mut c = Cursor::new(&bytes[..chunk_end]);
            c.pos = at + 6;
            match kind {
                CHUNK_LAYER => {
                    let flags = c.u16()?;
                    let layer_type = c.u16()?;
                    let level = usize::from(c.u16()?);
                    c.skip(2 + 2)?; // default width and height, unused
                    let blend = c.u16()?;
                    let opacity = c.u8()?;
                    c.skip(3)?;
                    let name = c.string()?;
                    let tileset = if layer_type == 2 { c.u32()? } else { 0 };
                    if layer_uuid {
                        c.skip(16)?;
                    }
                    groups.truncate(level);
                    let parent = groups.last().copied();
                    let background = flags & LAYER_BACKGROUND != 0;
                    let kind = match layer_type {
                        1 => Kind::Group,
                        2 => Kind::Tilemap,
                        _ => Kind::Image,
                    };
                    let opacity = if layer_opacity_valid && !background {
                        opacity
                    } else {
                        255
                    };
                    let visible = flags & LAYER_VISIBLE != 0;
                    let blend = BlendMode::from_code(blend).unwrap_or_else(|| {
                        notes.push(format!(
                            "layer {name} uses blend mode {blend}, which this reader does not know; it is drawn as Normal"
                        ));
                        BlendMode::Normal
                    });
                    if kind == Kind::Group {
                        groups.push(layers.len());
                    }
                    layers.push(Layer {
                        name,
                        parent,
                        kind,
                        visible,
                        opacity,
                        blend,
                        background,
                        reference: flags & LAYER_REFERENCE != 0,
                        tileset,
                    });
                    store.push(Vec::new());
                }
                CHUNK_CEL => {
                    let layer = usize::from(c.u16()?);
                    let x = i32::from(c.i16()?);
                    let y = i32::from(c.i16()?);
                    let opacity = c.u8()?;
                    let cel_type = c.u16()?;
                    let z = c.i16()?;
                    c.skip(5)?;
                    if layer >= layers.len() {
                        return Err(bad(format!("a cel in frame {frame} names a missing layer")));
                    }
                    let image = match cel_type {
                        0 | 2 => {
                            let w = u32::from(c.u16()?);
                            let h = u32::from(c.u16()?);
                            let count = u64::from(w) * u64::from(h);
                            if count > MAX_SHEET_PIXELS || stored_pixels + count > MAX_CEL_STORE {
                                return Err(bad("the cels are larger than the packer accepts"));
                            }
                            let len = count as usize * depth.bytes();
                            let raw = if cel_type == 0 {
                                c.take(len)?.to_vec()
                            } else {
                                inflate(c.rest(), len)?
                            };
                            stored_pixels += count;
                            let pixels = decode_pixels(
                                &raw,
                                depth,
                                &palette,
                                transparent_index,
                                layers[layer].background,
                            );
                            Some(Rc::new(Image {
                                width: w,
                                height: h,
                                pixels,
                            }))
                        }
                        1 => {
                            let linked = usize::from(c.u16()?);
                            let source = store[layer].get(linked).cloned().flatten();
                            Some(source.ok_or_else(|| {
                                bad(format!(
                                    "a linked cel in frame {frame} points at frame {linked}, which has no cel on that layer"
                                ))
                            })?)
                        }
                        3 if layers[layer].kind == Kind::Tilemap => {
                            let cols = u32::from(c.u16()?);
                            let rows = u32::from(c.u16()?);
                            let bytes_per_tile = match c.u16()? {
                                8 => 1,
                                16 => 2,
                                32 => 4,
                                bits => {
                                    return Err(bad(format!(
                                        "a tilemap cel in frame {frame} has {bits} bits per tile"
                                    )))
                                }
                            };
                            let id_mask = c.u32()?;
                            let x_flip = c.u32()?;
                            let y_flip = c.u32()?;
                            let d_flip = c.u32()?;
                            c.skip(10)?;
                            let count = u64::from(cols) * u64::from(rows);
                            if count > MAX_SHEET_PIXELS {
                                return Err(bad("the cels are larger than the packer accepts"));
                            }
                            let raw = inflate(c.rest(), count as usize * bytes_per_tile)?;
                            let tiles = raw
                                .chunks_exact(bytes_per_tile)
                                .map(|b| match *b {
                                    [v] => u32::from(v),
                                    [a, b] => u32::from(u16::from_le_bytes([a, b])),
                                    [a, b, c, d] => u32::from_le_bytes([a, b, c, d]),
                                    _ => 0,
                                })
                                .collect();
                            tilemaps.push(Tilemap {
                                layer,
                                x,
                                y,
                                opacity,
                                z,
                                cols,
                                rows,
                                tiles,
                                id_mask,
                                x_flip,
                                y_flip,
                                d_flip,
                            });
                            None
                        }
                        // Unknown cel types, and tilemap cels on other layers.
                        _ => None,
                    };
                    if let Some(image) = image {
                        let slot = &mut store[layer];
                        slot.resize(frame + 1, None);
                        slot[frame] = Some(image.clone());
                        cels.push(Cel {
                            layer,
                            x,
                            y,
                            opacity,
                            z,
                            image,
                        });
                    }
                }
                CHUNK_PALETTE => {
                    let size = c.u32()? as usize;
                    let first = c.u32()? as usize;
                    let last = c.u32()? as usize;
                    c.skip(8)?;
                    if size > 65_536 || first > last || last >= size.max(1) {
                        return Err(bad("the palette chunk is not valid"));
                    }
                    if palette.len() < size {
                        palette.resize(size, Rgba::CLEAR);
                    }
                    for slot in &mut palette[first..=last] {
                        let entry_flags = c.u16()?;
                        *slot = Rgba::new(c.u8()?, c.u8()?, c.u8()?, c.u8()?);
                        if entry_flags & 1 != 0 {
                            c.string()?;
                        }
                    }
                }
                CHUNK_OLD_PALETTE | CHUNK_OLD_PALETTE_64 => {
                    let packets = c.u16()?;
                    let mut index = 0usize;
                    for _ in 0..packets {
                        index += usize::from(c.u8()?);
                        let count = match c.u8()? {
                            0 => 256,
                            n => usize::from(n),
                        };
                        for _ in 0..count {
                            let mut rgb = [c.u8()?, c.u8()?, c.u8()?];
                            if kind == CHUNK_OLD_PALETTE_64 {
                                rgb = rgb.map(|v| ((u32::from(v.min(63)) * 255 + 31) / 63) as u8);
                            }
                            if let Some(slot) = palette.get_mut(index) {
                                *slot = Rgba::new(rgb[0], rgb[1], rgb[2], 255);
                            }
                            index += 1;
                        }
                    }
                }
                CHUNK_TILESET => {
                    let id = c.u32()?;
                    let set_flags = c.u32()?;
                    let count = c.u32()?;
                    let tile_w = u32::from(c.u16()?);
                    let tile_h = u32::from(c.u16()?);
                    c.skip(2 + 14)?; // base index (for display only), reserved
                    c.string()?;
                    if set_flags & TILESET_EXTERNAL != 0 {
                        c.skip(4 + 4)?; // external file and its tileset id
                    }
                    let pixels = if set_flags & TILESET_INSIDE != 0 {
                        let len = c.u32()? as usize;
                        let total = u64::from(tile_w) * u64::from(tile_h) * u64::from(count);
                        if total > MAX_SHEET_PIXELS || stored_pixels + total > MAX_CEL_STORE {
                            return Err(bad("a tileset is larger than the packer accepts"));
                        }
                        let raw = inflate(c.take(len)?, total as usize * depth.bytes())?;
                        stored_pixels += total;
                        Some(decode_pixels(
                            &raw,
                            depth,
                            &palette,
                            transparent_index,
                            false,
                        ))
                    } else {
                        None
                    };
                    tilesets.insert(
                        id,
                        Tileset {
                            tile_w,
                            tile_h,
                            count,
                            zero_empty: set_flags & TILESET_ZERO_EMPTY != 0,
                            pixels,
                        },
                    );
                }
                CHUNK_TAGS => {
                    let count = c.u16()?;
                    c.skip(8)?;
                    for _ in 0..count {
                        let from = usize::from(c.u16()?);
                        let to = usize::from(c.u16()?);
                        // Unknown directions play forward, as Aseprite does.
                        let direction = Direction::from_code(c.u8()?).unwrap_or_default();
                        let repeat = c.u16()?;
                        c.skip(6 + 3 + 1)?;
                        let name = c.string()?;
                        if from > to || to >= frame_count {
                            return Err(bad(format!("tag {name} has frames outside the file")));
                        }
                        tags.push(Tag {
                            name,
                            from,
                            to,
                            direction,
                            repeat,
                        });
                    }
                }
                _ => {}
            }
            at = chunk_end;
        }
        for map in tilemaps {
            let layer = &layers[map.layer];
            let tileset = tilesets.get(&layer.tileset).ok_or_else(|| {
                bad(format!(
                    "tilemap layer {} uses tileset {}, which the file does not have",
                    layer.name, layer.tileset
                ))
            })?;
            let Some(image) =
                draw_tilemap(&map, tileset, &layer.name, &mut notes, &mut stored_pixels)?
            else {
                continue;
            };
            let slot = &mut store[map.layer];
            slot.resize(frame + 1, None);
            slot[frame] = Some(image.clone());
            cels.push(Cel {
                layer: map.layer,
                x: map.x,
                y: map.y,
                opacity: map.opacity,
                z: map.z,
                image,
            });
        }
        frames.push((
            composite(width, height, &layers, &cels, group_opacity_valid),
            duration,
        ));
        pos = frame_end;
    }
    notes.dedup();
    Ok(AseFile {
        width,
        height,
        frames,
        tags,
        notes,
    })
}

/// Draw a tilemap cel from its tileset. `None` when there is nothing to draw.
fn draw_tilemap(
    map: &Tilemap,
    tileset: &Tileset,
    layer_name: &str,
    notes: &mut Vec<String>,
    stored_pixels: &mut u64,
) -> Result<Option<Rc<Image>>> {
    let Some(tiles) = &tileset.pixels else {
        notes.push(format!(
            "tilemap layer {layer_name} uses a tileset kept in another file; it is skipped"
        ));
        return Ok(None);
    };
    let (tw, th) = (tileset.tile_w, tileset.tile_h);
    let width = u64::from(map.cols) * u64::from(tw);
    let height = u64::from(map.rows) * u64::from(th);
    if width == 0 || height == 0 {
        return Ok(None);
    }
    let count = width * height;
    if count > MAX_SHEET_PIXELS || *stored_pixels + count > MAX_CEL_STORE {
        return Err(bad("the cels are larger than the packer accepts"));
    }
    *stored_pixels += count;
    let empty = if tileset.zero_empty { 0 } else { map.id_mask };
    let mut pixels = vec![Rgba::CLEAR; count as usize];
    let mut missing = false;
    for (i, &value) in map.tiles.iter().enumerate() {
        let id = value & map.id_mask;
        if id == empty {
            continue;
        }
        if id >= tileset.count {
            missing = true;
            continue;
        }
        let (col, row) = (i as u32 % map.cols, i as u32 / map.cols);
        let (fx, fy) = (value & map.x_flip != 0, value & map.y_flip != 0);
        let fd = value & map.d_flip != 0 && tw == th;
        for dy in 0..th {
            for dx in 0..tw {
                // Undo the flips in the reverse order they were made.
                let sy = if fy { th - 1 - dy } else { dy };
                let sx = if fx { tw - 1 - dx } else { dx };
                let (sx, sy) = if fd { (sy, sx) } else { (sx, sy) };
                let src = ((id * th + sy) as usize) * tw as usize + sx as usize;
                let dst = (row * th + dy) as usize * width as usize + (col * tw + dx) as usize;
                pixels[dst] = tiles[src];
            }
        }
    }
    if missing {
        notes.push(format!(
            "tilemap layer {layer_name} uses tiles its tileset does not have; they are drawn empty"
        ));
    }
    Ok(Some(Rc::new(Image {
        width: width as u32,
        height: height as u32,
        pixels,
    })))
}

/// zlib data that must inflate to exactly `len` bytes.
fn inflate(data: &[u8], len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len);
    flate2::read::ZlibDecoder::new(data)
        .take(len as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|err| bad(format!("a compressed cel is damaged: {err}")))?;
    if out.len() != len {
        return Err(bad("a compressed cel has the wrong number of pixels"));
    }
    Ok(out)
}

fn decode_pixels(
    raw: &[u8],
    depth: Depth,
    palette: &[Rgba],
    transparent_index: u8,
    background: bool,
) -> Vec<Rgba> {
    match depth {
        Depth::Rgba => raw
            .chunks_exact(4)
            .map(|p| Rgba::new(p[0], p[1], p[2], p[3]))
            .collect(),
        Depth::Grayscale => raw
            .chunks_exact(2)
            .map(|p| Rgba::new(p[0], p[0], p[0], p[1]))
            .collect(),
        Depth::Indexed => raw
            .iter()
            .map(|&i| {
                if i == transparent_index && !background {
                    Rgba::CLEAR
                } else {
                    palette.get(usize::from(i)).copied().unwrap_or(Rgba::CLEAR)
                }
            })
            .collect(),
    }
}

/// Draw a frame: the top level of the layer tree onto a clear canvas.
fn composite(
    width: u32,
    height: u32,
    layers: &[Layer],
    cels: &[Cel],
    group_blend: bool,
) -> Vec<Rgba> {
    let mut out = vec![Rgba::CLEAR; width as usize * height as usize];
    draw_children(None, width, height, layers, cels, group_blend, &mut out);
    out
}

/// Draw the children of `parent` (the top level for `None`) onto `out`,
/// bottom to top. A cel's place among them is its layer index plus its
/// z-index, the lower z-index first on a tie; a group sits at its own index.
///
/// When the file says group opacity and blend mode are valid, a group is
/// drawn into its own buffer, which is then blended onto `out` with the
/// group's opacity and mode, as Aseprite composites groups. Otherwise a
/// group's children draw straight onto `out`.
fn draw_children(
    parent: Option<usize>,
    width: u32,
    height: u32,
    layers: &[Layer],
    cels: &[Cel],
    group_blend: bool,
    out: &mut [Rgba],
) {
    enum Item<'a> {
        Cel(&'a Cel),
        Group(usize),
    }
    let mut items: Vec<((i64, i16), Item)> = Vec::new();
    for cel in cels {
        let layer = &layers[cel.layer];
        if layer.parent == parent && layer.kind != Kind::Group && layer.visible && !layer.reference
        {
            items.push(((cel.layer as i64 + i64::from(cel.z), cel.z), Item::Cel(cel)));
        }
    }
    for (index, layer) in layers.iter().enumerate() {
        if layer.parent == parent && layer.kind == Kind::Group && layer.visible {
            items.push(((index as i64, 0), Item::Group(index)));
        }
    }
    items.sort_by_key(|(key, _)| *key);
    for (_, item) in items {
        match item {
            Item::Cel(cel) => {
                let layer = &layers[cel.layer];
                draw_cel(
                    cel,
                    mul255(layer.opacity, cel.opacity),
                    layer.blend,
                    width,
                    height,
                    out,
                );
            }
            Item::Group(index) if group_blend => {
                let mut group = vec![Rgba::CLEAR; out.len()];
                draw_children(
                    Some(index),
                    width,
                    height,
                    layers,
                    cels,
                    group_blend,
                    &mut group,
                );
                let layer = &layers[index];
                for (dst, src) in out.iter_mut().zip(group) {
                    *dst = blend(*dst, src, layer.opacity, layer.blend);
                }
            }
            Item::Group(index) => {
                draw_children(Some(index), width, height, layers, cels, group_blend, out)
            }
        }
    }
}

fn draw_cel(cel: &Cel, opacity: u8, mode: BlendMode, width: u32, height: u32, out: &mut [Rgba]) {
    let image = &cel.image;
    for row in 0..image.height as i32 {
        let ty = cel.y + row;
        if ty < 0 || ty >= height as i32 {
            continue;
        }
        for col in 0..image.width as i32 {
            let tx = cel.x + col;
            if tx < 0 || tx >= width as i32 {
                continue;
            }
            let src = image.pixels[row as usize * image.width as usize + col as usize];
            let dst = &mut out[ty as usize * width as usize + tx as usize];
            *dst = blend(*dst, src, opacity, mode);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! A small writer for test files, following the same specification.

    use std::io::Write;

    use oxypixel_core::source_over;

    use super::*;

    pub(crate) struct Chunk(u16, Vec<u8>);

    fn string(out: &mut Vec<u8>, text: &str) {
        out.extend((text.len() as u16).to_le_bytes());
        out.extend(text.as_bytes());
    }

    pub(crate) fn layer(
        flags: u16,
        kind: u16,
        level: u16,
        blend: u16,
        opacity: u8,
        name: &str,
    ) -> Chunk {
        let mut d = Vec::new();
        for v in [flags, kind, level, 0, 0, blend] {
            d.extend(v.to_le_bytes());
        }
        d.push(opacity);
        d.extend([0; 3]);
        string(&mut d, name);
        Chunk(CHUNK_LAYER, d)
    }

    fn tilemap_layer(name: &str, tileset: u32) -> Chunk {
        let Chunk(kind, mut d) = layer(LAYER_VISIBLE, 2, 0, 0, 255, name);
        d.extend(tileset.to_le_bytes());
        Chunk(kind, d)
    }

    fn zlib(raw: &[u8]) -> Vec<u8> {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(raw).unwrap();
        z.finish().unwrap()
    }

    /// A tileset kept in the file: `raw` is the tiles stacked top to bottom.
    fn tileset(id: u32, tile: u16, count: u32, raw: &[u8]) -> Chunk {
        let mut d = Vec::new();
        d.extend(id.to_le_bytes());
        d.extend((TILESET_INSIDE | TILESET_ZERO_EMPTY).to_le_bytes());
        d.extend(count.to_le_bytes());
        d.extend(tile.to_le_bytes());
        d.extend(tile.to_le_bytes());
        d.extend(1i16.to_le_bytes());
        d.extend([0; 14]);
        string(&mut d, "tiles");
        let packed = zlib(raw);
        d.extend((packed.len() as u32).to_le_bytes());
        d.extend(packed);
        Chunk(CHUNK_TILESET, d)
    }

    fn external_tileset(id: u32) -> Chunk {
        let mut d = Vec::new();
        d.extend(id.to_le_bytes());
        d.extend((TILESET_EXTERNAL | TILESET_ZERO_EMPTY).to_le_bytes());
        d.extend(4u32.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(1i16.to_le_bytes());
        d.extend([0; 14]);
        string(&mut d, "elsewhere");
        d.extend([0; 8]);
        Chunk(CHUNK_TILESET, d)
    }

    const X_FLIP: u32 = 0x8000_0000;
    const Y_FLIP: u32 = 0x4000_0000;
    const D_FLIP: u32 = 0x2000_0000;

    fn cel_tilemap(layer: u16, x: i16, cols: u16, rows: u16, bits: u16, tiles: &[u32]) -> Chunk {
        let mut d = cel_head(layer, x, 0, 255, 3, 0);
        for v in [cols, rows, bits] {
            d.extend(v.to_le_bytes());
        }
        for mask in [0x1fff_ffff, X_FLIP, Y_FLIP, D_FLIP] {
            d.extend(u32::to_le_bytes(mask));
        }
        d.extend([0; 10]);
        let mut raw = Vec::new();
        for &t in tiles {
            raw.extend(&t.to_le_bytes()[..usize::from(bits / 8)]);
        }
        d.extend(zlib(&raw));
        Chunk(CHUNK_CEL, d)
    }

    fn cel_head(layer: u16, x: i16, y: i16, opacity: u8, kind: u16, z: i16) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(layer.to_le_bytes());
        d.extend(x.to_le_bytes());
        d.extend(y.to_le_bytes());
        d.push(opacity);
        d.extend(kind.to_le_bytes());
        d.extend(z.to_le_bytes());
        d.extend([0; 5]);
        d
    }

    pub(crate) fn cel(
        layer: u16,
        x: i16,
        y: i16,
        opacity: u8,
        w: u16,
        h: u16,
        raw: &[u8],
    ) -> Chunk {
        let mut d = cel_head(layer, x, y, opacity, 0, 0);
        d.extend(w.to_le_bytes());
        d.extend(h.to_le_bytes());
        d.extend(raw);
        Chunk(CHUNK_CEL, d)
    }

    fn cel_z(layer: u16, z: i16, w: u16, h: u16, raw: &[u8]) -> Chunk {
        let mut d = cel_head(layer, 0, 0, 255, 0, z);
        d.extend(w.to_le_bytes());
        d.extend(h.to_le_bytes());
        d.extend(raw);
        Chunk(CHUNK_CEL, d)
    }

    fn cel_zlib(layer: u16, x: i16, y: i16, w: u16, h: u16, raw: &[u8]) -> Chunk {
        let mut d = cel_head(layer, x, y, 255, 2, 0);
        d.extend(w.to_le_bytes());
        d.extend(h.to_le_bytes());
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(raw).unwrap();
        d.extend(z.finish().unwrap());
        Chunk(CHUNK_CEL, d)
    }

    fn cel_link(layer: u16, frame: u16) -> Chunk {
        let mut d = cel_head(layer, 0, 0, 255, 1, 0);
        d.extend(frame.to_le_bytes());
        Chunk(CHUNK_CEL, d)
    }

    pub(crate) fn tags(list: &[(u16, u16, u8, u16, &str)]) -> Chunk {
        let mut d = Vec::new();
        d.extend((list.len() as u16).to_le_bytes());
        d.extend([0; 8]);
        for (from, to, dir, repeat, name) in list {
            d.extend(from.to_le_bytes());
            d.extend(to.to_le_bytes());
            d.push(*dir);
            d.extend(repeat.to_le_bytes());
            d.extend([0; 6 + 3 + 1]);
            string(&mut d, name);
        }
        Chunk(CHUNK_TAGS, d)
    }

    fn palette(colors: &[[u8; 4]]) -> Chunk {
        let mut d = Vec::new();
        d.extend((colors.len() as u32).to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend((colors.len() as u32 - 1).to_le_bytes());
        d.extend([0; 8]);
        for c in colors {
            d.extend(0u16.to_le_bytes());
            d.extend(c);
        }
        Chunk(CHUNK_PALETTE, d)
    }

    /// A whole file: `frames` holds each frame's duration and chunks.
    pub(crate) fn file(
        w: u16,
        h: u16,
        depth: u16,
        transparent: u8,
        frames: Vec<(u16, Vec<Chunk>)>,
    ) -> Vec<u8> {
        file_flags(
            FLAG_LAYER_OPACITY | FLAG_GROUP_OPACITY,
            w,
            h,
            depth,
            transparent,
            frames,
        )
    }

    fn file_flags(
        flags: u32,
        w: u16,
        h: u16,
        depth: u16,
        transparent: u8,
        frames: Vec<(u16, Vec<Chunk>)>,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        for (duration, chunks) in &frames {
            let mut data = Vec::new();
            for Chunk(kind, bytes) in chunks {
                data.extend((bytes.len() as u32 + 6).to_le_bytes());
                data.extend(kind.to_le_bytes());
                data.extend(bytes);
            }
            body.extend((data.len() as u32 + 16).to_le_bytes());
            body.extend(FRAME_MAGIC.to_le_bytes());
            body.extend((chunks.len() as u16).to_le_bytes());
            body.extend(duration.to_le_bytes());
            body.extend([0; 2]);
            body.extend((chunks.len() as u32).to_le_bytes());
            body.extend(data);
        }
        let mut out = Vec::new();
        out.extend((body.len() as u32 + 128).to_le_bytes());
        out.extend(HEADER_MAGIC.to_le_bytes());
        out.extend((frames.len() as u16).to_le_bytes());
        out.extend(w.to_le_bytes());
        out.extend(h.to_le_bytes());
        out.extend(depth.to_le_bytes());
        out.extend(flags.to_le_bytes());
        out.extend(100u16.to_le_bytes());
        out.extend([0; 8]);
        out.push(transparent);
        out.extend([0; 3]);
        out.extend(0u16.to_le_bytes());
        out.extend([1, 1]);
        out.extend([0; 8]);
        out.extend([0; 84]);
        assert_eq!(out.len(), 128);
        out.extend(body);
        out
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    fn px(c: [u8; 4]) -> Rgba {
        Rgba::new(c[0], c[1], c[2], c[3])
    }

    #[test]
    fn layers_stack_with_opacity_and_compressed_cels() {
        let bytes = file(
            2,
            2,
            32,
            0,
            vec![(
                120,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "base"),
                    layer(LAYER_VISIBLE, 0, 0, 0, 128, "glaze"),
                    cel(0, 0, 0, 255, 2, 2, &RED.repeat(4)),
                    cel_zlib(1, 1, 1, 1, 1, &BLUE),
                ],
            )],
        );
        let ase = read_aseprite(&bytes).unwrap();
        assert_eq!((ase.width, ase.height, ase.frames.len()), (2, 2, 1));
        let (pixels, duration) = &ase.frames[0];
        assert_eq!(*duration, 120);
        assert_eq!(pixels[0], px(RED));
        assert_eq!(pixels[3], source_over(px(RED), px(BLUE), 128));
        assert!(ase.notes.is_empty());
    }

    #[test]
    fn hidden_groups_reference_layers_and_blend_modes() {
        let bytes = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 3, 255, "multiply"),
                    layer(0, 1, 0, 0, 255, "hidden group"),
                    layer(LAYER_VISIBLE, 0, 1, 0, 255, "inside"),
                    layer(LAYER_VISIBLE | LAYER_REFERENCE, 0, 0, 0, 255, "ref"),
                    cel(0, 0, 0, 255, 1, 1, &RED),
                    cel(2, 0, 0, 255, 1, 1, &BLUE),
                    cel(3, 0, 0, 255, 1, 1, &BLUE),
                ],
            )],
        );
        // Multiply over a clear canvas is the color itself; the hidden group
        // and the reference layer are not drawn.
        let ase = read_aseprite(&bytes).unwrap();
        assert_eq!(ase.frames[0].0, vec![px(RED)]);
        assert!(ase.notes.is_empty(), "{:?}", ase.notes);
    }

    #[test]
    fn blend_modes_apply_and_unknown_ones_are_noted() {
        let gray = [128, 128, 128, 255];
        let bytes = file(
            2,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "base"),
                    layer(LAYER_VISIBLE, 0, 0, 1, 255, "multiply"),
                    layer(LAYER_VISIBLE, 0, 0, 99, 255, "future"),
                    cel(0, 0, 0, 255, 2, 1, &gray.repeat(2)),
                    cel(1, 0, 0, 255, 1, 1, &RED),
                    cel(2, 1, 0, 255, 1, 1, &BLUE),
                ],
            )],
        );
        let ase = read_aseprite(&bytes).unwrap();
        assert_eq!(ase.frames[0].0[0], Rgba::new(128, 0, 0, 255));
        // The unknown mode 99 is drawn as Normal, with a note.
        assert_eq!(ase.frames[0].0[1], px(BLUE));
        assert_eq!(ase.notes.len(), 1);
        assert!(ase.notes[0].contains("future") && ase.notes[0].contains("99"));
    }

    #[test]
    fn a_group_with_its_own_blend_mode_is_drawn_as_one_image() {
        // A gray base, then a Multiply group holding white and red Normal
        // layers. With group blending the group is drawn alone (red) and
        // multiplied onto gray: dark red. Without it, the group's mode is
        // ignored and its Normal children cover the base: plain red.
        let gray = [128, 128, 128, 255];
        let white = [255, 255, 255, 255];
        let chunks = || {
            vec![
                layer(LAYER_VISIBLE, 0, 0, 0, 255, "base"),
                layer(LAYER_VISIBLE, 1, 0, 1, 255, "group"),
                layer(LAYER_VISIBLE, 0, 1, 0, 255, "white"),
                layer(LAYER_VISIBLE, 0, 1, 0, 255, "red"),
                cel(0, 0, 0, 255, 1, 1, &gray),
                cel(2, 0, 0, 255, 1, 1, &white),
                cel(3, 0, 0, 255, 1, 1, &RED),
            ]
        };
        let grouped = read_aseprite(&file(1, 1, 32, 0, vec![(100, chunks())])).unwrap();
        assert_eq!(grouped.frames[0].0[0], Rgba::new(128, 0, 0, 255));
        let passed = file_flags(FLAG_LAYER_OPACITY, 1, 1, 32, 0, vec![(100, chunks())]);
        assert_eq!(read_aseprite(&passed).unwrap().frames[0].0[0], px(RED));
    }

    #[test]
    fn group_opacity_multiplies_and_z_index_reorders() {
        let half = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 1, 0, 0, 128, "group"),
                    layer(LAYER_VISIBLE, 0, 1, 0, 255, "child"),
                    cel(1, 0, 0, 255, 1, 1, &BLUE),
                ],
            )],
        );
        let ase = read_aseprite(&half).unwrap();
        assert_eq!(ase.frames[0].0[0], source_over(Rgba::CLEAR, px(BLUE), 128));
        // Without the group flag, or on a background layer, opacity is ignored.
        let chunks = || {
            vec![
                layer(LAYER_VISIBLE, 1, 0, 0, 128, "group"),
                layer(LAYER_VISIBLE, 0, 1, 0, 255, "child"),
                layer(LAYER_VISIBLE | LAYER_BACKGROUND, 0, 0, 0, 10, "bg"),
                cel(1, 0, 0, 255, 1, 1, &BLUE),
                cel(2, 1, 0, 255, 1, 1, &RED),
            ]
        };
        let plain = file_flags(FLAG_LAYER_OPACITY, 2, 1, 32, 0, vec![(100, chunks())]);
        assert_eq!(
            read_aseprite(&plain).unwrap().frames[0].0,
            vec![px(BLUE), px(RED)]
        );

        // Layer 0's cel has z-index 2, so it is drawn over layer 1.
        let raised = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "low"),
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "high"),
                    cel_z(0, 2, 1, 1, &RED),
                    cel_z(1, 0, 1, 1, &BLUE),
                ],
            )],
        );
        assert_eq!(read_aseprite(&raised).unwrap().frames[0].0[0], px(RED));
    }

    #[test]
    fn linked_cels_repeat_an_earlier_frame() {
        let bytes = file(
            1,
            1,
            32,
            0,
            vec![
                (
                    100,
                    vec![
                        layer(LAYER_VISIBLE, 0, 0, 0, 255, "a"),
                        cel(0, 0, 0, 255, 1, 1, &BLUE),
                    ],
                ),
                (80, vec![cel_link(0, 0)]),
            ],
        );
        let ase = read_aseprite(&bytes).unwrap();
        assert_eq!(ase.frames[1], (vec![px(BLUE)], 80));
        let broken = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![layer(LAYER_VISIBLE, 0, 0, 0, 255, "a"), cel_link(0, 3)],
            )],
        );
        assert!(read_aseprite(&broken).unwrap_err().0.contains("linked cel"));
    }

    #[test]
    fn indexed_and_grayscale_pixels() {
        let colors = [[9, 9, 9, 255], RED, BLUE];
        let indexed = file(
            3,
            1,
            8,
            0,
            vec![(
                100,
                vec![
                    palette(&colors),
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "art"),
                    cel(0, 0, 0, 255, 3, 1, &[0, 1, 2]),
                ],
            )],
        );
        let ase = read_aseprite(&indexed).unwrap();
        assert_eq!(ase.frames[0].0, vec![Rgba::CLEAR, px(RED), px(BLUE)]);
        // On a background layer the transparent index is a real color.
        let background = file(
            1,
            1,
            8,
            0,
            vec![(
                100,
                vec![
                    palette(&colors),
                    layer(LAYER_VISIBLE | LAYER_BACKGROUND, 0, 0, 0, 255, "bg"),
                    cel(0, 0, 0, 255, 1, 1, &[0]),
                ],
            )],
        );
        assert_eq!(
            read_aseprite(&background).unwrap().frames[0].0[0],
            px(colors[0])
        );
        let gray = file(
            1,
            1,
            16,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "g"),
                    cel(0, 0, 0, 255, 1, 1, &[200, 255]),
                ],
            )],
        );
        assert_eq!(
            read_aseprite(&gray).unwrap().frames[0].0[0],
            Rgba::new(200, 200, 200, 255)
        );
    }

    /// Tile 1 of a 2×2 tileset: red in the top-right pixel, the rest clear.
    fn corner_tiles() -> Vec<u8> {
        let mut raw = [0u8; 4].repeat(4); // tile 0, empty
        raw.extend([[0; 4], RED, [0; 4], [0; 4]].concat());
        raw
    }

    #[test]
    fn tilemaps_draw_their_tiles_with_flips() {
        let tiles = [
            1,
            1 | X_FLIP,
            1 | Y_FLIP,
            1 | D_FLIP,
            1 | D_FLIP | X_FLIP,
            0,
            9,
        ];
        let bytes = file(
            14,
            2,
            32,
            0,
            vec![(
                100,
                vec![
                    tileset(3, 2, 2, &corner_tiles()),
                    tilemap_layer("map", 3),
                    cel_tilemap(0, 0, 7, 1, 32, &tiles),
                ],
            )],
        );
        let ase = read_aseprite(&bytes).unwrap();
        let pixels = &ase.frames[0].0;
        let red: Vec<(usize, usize)> = (0..pixels.len())
            .filter(|&i| pixels[i] == px(RED))
            .map(|i| (i % 14, i / 14))
            .collect();
        // Each tile is 2 pixels wide. Plain: top right. X flip: top left.
        // Y flip: bottom right. Diagonal: bottom left. Diagonal then x:
        // bottom right. Tile 0 and the missing tile 9 are empty.
        assert_eq!(red, vec![(1, 0), (2, 0), (5, 1), (6, 1), (9, 1)]);
        // Cutting or scribbling on the file fails cleanly.
        for len in 0..bytes.len() {
            let _ = read_aseprite(&bytes[..len]);
        }
        for at in 128..bytes.len() {
            let mut changed = bytes.clone();
            changed[at] ^= 0xff;
            let _ = read_aseprite(&changed);
        }
        assert_eq!(ase.notes.len(), 1);
        assert!(ase.notes[0].contains("map"), "{:?}", ase.notes);
    }

    #[test]
    fn small_tiles_linked_tilemaps_and_external_tilesets() {
        // 8-bit tiles, an offset cel, and a linked cel in the next frame.
        let bytes = file(
            3,
            2,
            32,
            0,
            vec![
                (
                    100,
                    vec![
                        tileset(0, 2, 2, &corner_tiles()),
                        tilemap_layer("map", 0),
                        cel_tilemap(0, 1, 1, 1, 8, &[1]),
                    ],
                ),
                (100, vec![cel_link(0, 0)]),
            ],
        );
        let ase = read_aseprite(&bytes).unwrap();
        // The linked cel has its own position, x 0, so its red is one left.
        for ((pixels, _), at) in ase.frames.iter().zip([2, 1]) {
            assert_eq!(pixels[at], px(RED));
            assert_eq!(pixels.iter().filter(|p| **p == px(RED)).count(), 1);
        }
        assert!(ase.notes.is_empty(), "{:?}", ase.notes);

        let external = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    external_tileset(0),
                    tilemap_layer("outside", 0),
                    cel_tilemap(0, 0, 1, 1, 16, &[1]),
                ],
            )],
        );
        let ase = read_aseprite(&external).unwrap();
        assert_eq!(ase.frames[0].0, vec![Rgba::CLEAR]);
        assert!(ase.notes[0].contains("another file"), "{:?}", ase.notes);
    }

    #[test]
    fn broken_tilemaps_are_errors() {
        let missing = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![tilemap_layer("map", 7), cel_tilemap(0, 0, 1, 1, 32, &[1])],
            )],
        );
        assert!(read_aseprite(&missing).unwrap_err().0.contains("tileset 7"));
        let odd_bits = file(
            1,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    tileset(0, 2, 2, &corner_tiles()),
                    tilemap_layer("map", 0),
                    cel_tilemap(0, 0, 1, 1, 24, &[1]),
                ],
            )],
        );
        assert!(read_aseprite(&odd_bits).unwrap_err().0.contains("24 bits"));
        let short = file(
            1,
            1,
            32,
            0,
            vec![(100, vec![tileset(0, 2, 3, &corner_tiles())])],
        );
        assert!(read_aseprite(&short)
            .unwrap_err()
            .0
            .contains("wrong number"));
    }

    #[test]
    fn tags_are_read_and_checked() {
        let bytes = file(
            1,
            1,
            32,
            0,
            vec![
                (
                    100,
                    vec![tags(&[(0, 1, 2, 3, "walk"), (1, 1, 0, 0, "idle")])],
                ),
                (100, vec![]),
            ],
        );
        let ase = read_aseprite(&bytes).unwrap();
        assert_eq!(
            ase.tags,
            vec![
                Tag {
                    name: "walk".into(),
                    from: 0,
                    to: 1,
                    direction: Direction::PingPong,
                    repeat: 3
                },
                Tag {
                    name: "idle".into(),
                    from: 1,
                    to: 1,
                    direction: Direction::Forward,
                    repeat: 0
                },
            ]
        );
        let outside = file(1, 1, 32, 0, vec![(100, vec![tags(&[(0, 4, 0, 0, "far")])])]);
        assert!(read_aseprite(&outside).unwrap_err().0.contains("far"));
    }

    #[test]
    fn damaged_files_are_errors_not_panics() {
        let good = file(
            2,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "a"),
                    cel_zlib(0, 0, 0, 2, 1, &RED.repeat(2)),
                ],
            )],
        );
        assert!(read_aseprite(&good).is_ok());
        // Every truncation fails cleanly.
        for len in 0..good.len() {
            let _ = read_aseprite(&good[..len]);
        }
        let mut magic = good.clone();
        magic[4] = 0;
        assert!(read_aseprite(&magic)
            .unwrap_err()
            .0
            .contains("not an Aseprite"));
        let short = file(
            2,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "a"),
                    cel_zlib(0, 0, 0, 2, 1, &RED),
                ],
            )],
        );
        assert!(read_aseprite(&short)
            .unwrap_err()
            .0
            .contains("wrong number"));
        let raw_short = file(
            2,
            1,
            32,
            0,
            vec![(
                100,
                vec![
                    layer(LAYER_VISIBLE, 0, 0, 0, 255, "a"),
                    cel(0, 0, 0, 255, 2, 1, &RED),
                ],
            )],
        );
        assert!(read_aseprite(&raw_short).is_err());
        let mut huge = good.clone();
        huge[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(read_aseprite(&huge).unwrap_err().0.contains("canvas"));
        let no_layer = file(
            1,
            1,
            32,
            0,
            vec![(100, vec![cel(0, 0, 0, 255, 1, 1, &RED)])],
        );
        assert!(read_aseprite(&no_layer)
            .unwrap_err()
            .0
            .contains("missing layer"));
        let mut long_chunk = good.clone();
        long_chunk[128 + 16..128 + 20].copy_from_slice(&9999u32.to_le_bytes());
        assert!(read_aseprite(&long_chunk).is_err());
    }
}
