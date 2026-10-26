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

use crate::color::{unpack, Rgba};
use crate::document::{Document, Frame, LayerInfo};
use crate::error::{Error, Result};
use crate::limits::{
    check_file_len, check_stack, clamp_name, FORMAT_VERSION, MAGIC, MAX_NAME_BYTES, MAX_TAGS,
};
use crate::tags::{FrameTag, LoopDirection};

/// Version 1: no sections.
const VERSION_1: u16 = 1;
/// Section kinds with this bit set must be understood; a reader that does not
/// know one rejects the file. Kinds without it may be skipped.
const SECTION_REQUIRED: u16 = 0x8000;
/// Animation tags. Optional: a reader that skips it loses only the tags.
const SECTION_TAGS: u16 = 0x0001;
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
            for px in cel {
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
    list
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
            name,
            visible,
            opacity,
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
            cels.push(pixels);
        }
        frames.push(Frame { duration_ms, cels });
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
                kind if kind & SECTION_REQUIRED != 0 => return Err(Error::RequiredSection(kind)),
                // An optional section from a newer writer: skip it.
                _ => {}
            }
        }
    }
    if !r.finished() {
        return Err(Error::TrailingBytes);
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
    fn file_size_cap_is_enforced_without_reading_pixels() {
        let err = check_file_len(MAX_FILE_BYTES + 1).unwrap_err();
        assert!(matches!(err, Error::Limit { .. }));
    }
}
