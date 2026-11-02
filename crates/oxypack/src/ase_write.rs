//! Write a document as an Aseprite `.aseprite` file.
//!
//! Written from Aseprite's published file format description
//! (`docs/ase-file-specs.md` in the Aseprite repository), like the reader
//! beside it. No Aseprite code is used. The file is RGBA (32-bit) with the
//! document's palette, layers (groups, visibility, opacity, blend modes),
//! frame durations, tags, and tilemap layers with their tilesets. Each image
//! cel is cropped to its visible pixels and zlib-compressed; clear cels are
//! left out. References are not part of the document and are not written.

use std::io::Write;
use std::path::Path;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use oxypixel_core::{Document, Rgba, TILE_FLIP_D, TILE_FLIP_X, TILE_FLIP_Y, TILE_ID_MASK};

const HEADER_MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;
const CHUNK_LAYER: u16 = 0x2004;
const CHUNK_CEL: u16 = 0x2005;
const CHUNK_TAGS: u16 = 0x2018;
const CHUNK_PALETTE: u16 = 0x2019;
const CHUNK_TILESET: u16 = 0x2023;

/// Layer opacity is valid, and so are group opacity and blend mode.
const HEADER_FLAGS: u32 = 1 | 2;
const LAYER_VISIBLE: u16 = 1;
const LAYER_EDITABLE: u16 = 2;
/// Tiles are in this file, and tile 0 is the empty tile.
const TILESET_FLAGS: u32 = 2 | 4;

const CEL_IMAGE: u16 = 2;
const CEL_TILEMAP: u16 = 3;

pub fn save_aseprite(doc: &Document, path: &Path) -> Result<(), String> {
    let bytes = write_aseprite(doc)?;
    std::fs::write(path, bytes).map_err(|err| format!("{}: {err}", path.display()))
}

/// The document as the bytes of an `.aseprite` file.
pub fn write_aseprite(doc: &Document) -> Result<Vec<u8>, String> {
    let (width, height) = (doc.width(), doc.height());
    let side = |n: u32| u16::try_from(n).map_err(|_| "the canvas is too large".to_string());
    let (w16, h16) = (side(width)?, side(height)?);
    let frames = doc.frame_count();
    let mut body = Vec::new();
    for frame in 0..frames {
        let mut chunks: Vec<(u16, Vec<u8>)> = Vec::new();
        if frame == 0 {
            chunks.push((CHUNK_PALETTE, palette_chunk(doc.palette())));
            for (id, set) in doc.tilesets().iter().enumerate() {
                chunks.push((CHUNK_TILESET, tileset_chunk(id as u32, set)?));
            }
            for (index, layer) in doc.layers().iter().enumerate() {
                chunks.push((CHUNK_LAYER, layer_chunk(doc, index, layer)));
            }
            if !doc.tags().is_empty() {
                chunks.push((CHUNK_TAGS, tags_chunk(doc)));
            }
        }
        for (index, layer) in doc.layers().iter().enumerate() {
            if layer.group {
                continue;
            }
            let cel = if let Some(set) = doc.layer_tileset(index) {
                let grid = doc.tile_grid(frame, index).unwrap_or_default();
                tilemap_cel(index, &doc.tilesets()[set], grid, width, height)?
            } else {
                let pixels = doc.cel(frame, index).map_err(|err| err.to_string())?;
                image_cel(index, pixels, width, height)?
            };
            if let Some(cel) = cel {
                chunks.push((CHUNK_CEL, cel));
            }
        }
        let duration = u16::try_from(doc.frame_duration(frame).unwrap_or(100)).unwrap_or(u16::MAX);
        let mut data = Vec::new();
        for (kind, bytes) in &chunks {
            put_u32(&mut data, bytes.len() as u32 + 6);
            put_u16(&mut data, *kind);
            data.extend(bytes);
        }
        put_u32(&mut body, data.len() as u32 + 16);
        put_u16(&mut body, FRAME_MAGIC);
        put_u16(&mut body, u16::try_from(chunks.len()).unwrap_or(u16::MAX));
        put_u16(&mut body, duration);
        body.extend([0; 2]);
        put_u32(&mut body, chunks.len() as u32);
        body.extend(data);
    }
    let mut out = Vec::with_capacity(128 + body.len());
    put_u32(&mut out, body.len() as u32 + 128);
    put_u16(&mut out, HEADER_MAGIC);
    put_u16(&mut out, frames as u16);
    put_u16(&mut out, w16);
    put_u16(&mut out, h16);
    put_u16(&mut out, 32);
    put_u32(&mut out, HEADER_FLAGS);
    put_u16(&mut out, 100); // speed, deprecated
    out.extend([0; 8]);
    out.push(0); // transparent index, for indexed files only
    out.extend([0; 3]);
    put_u16(&mut out, doc.palette().len() as u16);
    out.extend([1, 1]); // square pixels
    out.extend([0; 4]); // grid position
    let grid = doc
        .tilesets()
        .first()
        .map_or((16, 16), |s| (s.tile_width(), s.tile_height()));
    put_u16(&mut out, grid.0 as u16);
    put_u16(&mut out, grid.1 as u16);
    out.extend([0; 84]);
    debug_assert_eq!(out.len(), 128);
    out.extend(body);
    Ok(out)
}

fn palette_chunk(colors: &[Rgba]) -> Vec<u8> {
    let mut d = Vec::new();
    let n = colors.len().max(1) as u32;
    put_u32(&mut d, n);
    put_u32(&mut d, 0);
    put_u32(&mut d, n - 1);
    d.extend([0; 8]);
    for c in colors {
        put_u16(&mut d, 0);
        d.extend([c.r, c.g, c.b, c.a]);
    }
    if colors.is_empty() {
        put_u16(&mut d, 0);
        d.extend([0; 4]);
    }
    d
}

fn tileset_chunk(id: u32, set: &oxypixel_core::Tileset) -> Result<Vec<u8>, String> {
    let (tw, th) = (set.tile_width(), set.tile_height());
    let mut d = Vec::new();
    put_u32(&mut d, id);
    put_u32(&mut d, TILESET_FLAGS);
    put_u32(&mut d, set.len() as u32 + 1);
    put_u16(&mut d, tw as u16);
    put_u16(&mut d, th as u16);
    put_u16(&mut d, 1); // base index shown in Aseprite
    d.extend([0; 14]);
    put_string(&mut d, &set.name);
    // Tile 0 is the empty tile, then each tile, stacked top to bottom.
    let mut raw = vec![0u8; tw as usize * th as usize * 4];
    for id in 1..=set.len() as u32 {
        for p in set.tile(id).unwrap_or_default() {
            raw.extend([p.r, p.g, p.b, p.a]);
        }
    }
    let packed = zlib(&raw)?;
    put_u32(&mut d, packed.len() as u32);
    d.extend(packed);
    Ok(d)
}

fn layer_chunk(doc: &Document, index: usize, layer: &oxypixel_core::LayerInfo) -> Vec<u8> {
    let mut flags = LAYER_EDITABLE;
    if layer.visible {
        flags |= LAYER_VISIBLE;
    }
    let kind = if layer.group {
        1
    } else if layer.tileset.is_some() {
        2
    } else {
        0
    };
    let mut d = Vec::new();
    for v in [
        flags,
        kind,
        u16::from(layer.depth),
        0,
        0,
        layer.blend.code(),
    ] {
        put_u16(&mut d, v);
    }
    d.push(layer.opacity);
    d.extend([0; 3]);
    put_string(&mut d, &layer.name);
    if let Some(set) = doc.layer_tileset(index) {
        put_u32(&mut d, set as u32);
    }
    d
}

fn tags_chunk(doc: &Document) -> Vec<u8> {
    let tags = doc.tags();
    let mut d = Vec::new();
    put_u16(&mut d, tags.len() as u16);
    d.extend([0; 8]);
    for tag in tags {
        put_u16(&mut d, tag.from as u16);
        put_u16(&mut d, tag.to as u16);
        d.push(tag.direction.code());
        put_u16(&mut d, tag.repeat);
        d.extend([0; 6]);
        d.extend([0, 0, 0]); // tag color, deprecated
        d.push(0);
        put_string(&mut d, &tag.name);
    }
    d
}

fn cel_head(layer: usize, x: u32, y: u32, kind: u16) -> Vec<u8> {
    let mut d = Vec::new();
    put_u16(&mut d, layer as u16);
    put_u16(&mut d, x as u16);
    put_u16(&mut d, y as u16);
    d.push(255);
    put_u16(&mut d, kind);
    put_u16(&mut d, 0); // z-index
    d.extend([0; 5]);
    d
}

/// The visible part of a cel, or `None` when it is all clear.
fn image_cel(
    layer: usize,
    pixels: &[Rgba],
    width: u32,
    height: u32,
) -> Result<Option<Vec<u8>>, String> {
    let visible = |x: u32, y: u32| pixels[(y * width + x) as usize].a != 0;
    let rows: Vec<u32> = (0..height)
        .filter(|&y| (0..width).any(|x| visible(x, y)))
        .collect();
    let (Some(&top), Some(&bottom)) = (rows.first(), rows.last()) else {
        return Ok(None);
    };
    let cols: Vec<u32> = (0..width)
        .filter(|&x| (top..=bottom).any(|y| visible(x, y)))
        .collect();
    let (left, right) = (cols[0], cols[cols.len() - 1]);
    let (w, h) = (right - left + 1, bottom - top + 1);
    let mut raw = Vec::with_capacity((w * h * 4) as usize);
    for y in top..=bottom {
        for x in left..=right {
            let p = pixels[(y * width + x) as usize];
            raw.extend([p.r, p.g, p.b, p.a]);
        }
    }
    let mut d = cel_head(layer, left, top, CEL_IMAGE);
    put_u16(&mut d, w as u16);
    put_u16(&mut d, h as u16);
    d.extend(zlib(&raw)?);
    Ok(Some(d))
}

/// A tilemap cel covering the canvas from the top-left, or `None` when
/// every cell is empty.
fn tilemap_cel(
    layer: usize,
    set: &oxypixel_core::Tileset,
    grid: &[u32],
    width: u32,
    height: u32,
) -> Result<Option<Vec<u8>>, String> {
    if grid.iter().all(|&v| v == 0) {
        return Ok(None);
    }
    let (cols, rows) = set.grid_size(width, height);
    let mut d = cel_head(layer, 0, 0, CEL_TILEMAP);
    put_u16(&mut d, cols as u16);
    put_u16(&mut d, rows as u16);
    put_u16(&mut d, 32);
    // The same bits as OxyPixel's grid entries.
    for mask in [TILE_ID_MASK, TILE_FLIP_X, TILE_FLIP_Y, TILE_FLIP_D] {
        put_u32(&mut d, mask);
    }
    d.extend([0; 10]);
    let mut raw = Vec::with_capacity(grid.len() * 4);
    for &value in grid {
        raw.extend(value.to_le_bytes());
    }
    d.extend(zlib(&raw)?);
    Ok(Some(d))
}

fn zlib(raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(raw).map_err(|err| err.to_string())?;
    z.finish().map_err(|err| err.to_string())
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    let bytes = &text.as_bytes()[..text.len().min(u16::MAX as usize)];
    put_u16(out, bytes.len() as u16);
    out.extend(bytes);
}

#[cfg(test)]
mod tests {
    use oxypixel_core::{
        default_palette, BlendMode, Editor, FrameTag, LayerInfo, LoopDirection, MouseButton, Tool,
    };

    use super::*;
    use crate::aseprite::{read_aseprite, read_aseprite_document};

    fn layer(name: &str, f: impl FnOnce(&mut LayerInfo)) -> LayerInfo {
        let mut info = LayerInfo::new(name.to_string());
        f(&mut info);
        info
    }

    /// A hidden layer, then a group holding a faded Multiply layer, over two frames.
    fn sample() -> Document {
        let (w, h) = (8, 6);
        let layers = vec![
            layer("Back", |l| l.visible = false),
            layer("Group", |l| {
                l.group = true;
                l.opacity = 200;
                l.blend = BlendMode::Screen;
            }),
            layer("Inside", |l| {
                l.depth = 1;
                l.opacity = 180;
                l.blend = BlendMode::Multiply;
            }),
        ];
        let cel = |seed: u8| -> Vec<Rgba> {
            (0..w * h)
                .map(|i| match (i + u32::from(seed)) % 5 {
                    0 => Rgba::CLEAR,
                    n => Rgba::new(n as u8 * 40, seed, 200 - n as u8 * 30, 255 - n as u8 * 20),
                })
                .collect()
        };
        let clear = vec![Rgba::CLEAR; (w * h) as usize];
        let frames = vec![
            (100, vec![cel(1), clear.clone(), cel(2)]),
            (240, vec![cel(3), clear, cel(4)]),
        ];
        let tags = vec![FrameTag {
            name: "walk".into(),
            from: 0,
            to: 1,
            direction: LoopDirection::PingPong,
            repeat: 3,
        }];
        Document::from_layers(w, h, layers, frames, default_palette(), tags).unwrap()
    }

    #[test]
    fn a_document_survives_a_round_trip() {
        let doc = sample();
        let bytes = write_aseprite(&doc).unwrap();
        let (back, notes) = read_aseprite_document(&bytes).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((back.width(), back.height()), (doc.width(), doc.height()));
        assert_eq!(back.layers(), doc.layers());
        assert_eq!(back.palette(), doc.palette());
        assert_eq!(back.tags(), doc.tags());
        let flat = read_aseprite(&bytes).unwrap();
        for frame in 0..doc.frame_count() {
            assert_eq!(back.frame_duration(frame), doc.frame_duration(frame));
            for layer in 0..doc.layer_count() {
                assert_eq!(
                    back.cel(frame, layer).unwrap(),
                    doc.cel(frame, layer).unwrap()
                );
            }
            // The packer's own compositing reads the same picture.
            assert_eq!(
                flat.frames[frame].0,
                doc.composite(frame).unwrap(),
                "frame {frame}"
            );
        }
    }

    #[test]
    fn tilemaps_survive_a_round_trip() {
        let mut editor = Editor::new(20, 12).unwrap();
        editor.add_tilemap_layer(None, 8, 8).unwrap();
        let layer = editor.document().active_layer();
        editor.set_tool(Tool::Pencil);
        editor.pointer_down(1, 1, MouseButton::Primary, false);
        editor.pointer_up(1, 1, false);
        let mut doc = editor.document().clone();
        doc.set_tile(0, layer, 2, 1, 1 | TILE_FLIP_X | TILE_FLIP_D)
            .unwrap();
        let bytes = write_aseprite(&doc).unwrap();
        let (back, notes) = read_aseprite_document(&bytes).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(back.layers(), doc.layers());
        assert_eq!(back.tilesets(), doc.tilesets());
        assert_eq!(back.tile_grid(0, layer), doc.tile_grid(0, layer));
        assert_eq!(back.composite(0).unwrap(), doc.composite(0).unwrap());
    }

    #[test]
    fn clear_cels_are_left_out_and_others_cropped() {
        let clear = |w: u32, h: u32| vec![Rgba::CLEAR; (w * h) as usize];
        let make = |pixels: Vec<Rgba>| {
            Document::from_layers(
                64,
                64,
                vec![LayerInfo::new("Layer".into())],
                vec![(100, vec![pixels])],
                default_palette(),
                Vec::new(),
            )
            .unwrap()
        };
        let empty = write_aseprite(&make(clear(64, 64))).unwrap();
        assert!(
            empty.len() < 300,
            "{} bytes for a clear canvas",
            empty.len()
        );
        let mut pixels = clear(64, 64);
        pixels[10 * 64 + 20] = Rgba::new(1, 2, 3, 255);
        let one = write_aseprite(&make(pixels)).unwrap();
        let (back, _) = read_aseprite_document(&one).unwrap();
        assert_eq!(
            back.cel(0, 0).unwrap()[10 * 64 + 20],
            Rgba::new(1, 2, 3, 255)
        );
        assert!(one.len() < empty.len() + 64, "{} bytes", one.len());
    }
}
