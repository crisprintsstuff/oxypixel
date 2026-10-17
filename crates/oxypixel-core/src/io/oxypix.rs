//! Version 1 `.oxypix` files. Little-endian, uncompressed, no external schema.
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
//! ```

use std::path::Path;

use crate::color::{unpack, Rgba};
use crate::document::{Document, Frame, LayerInfo};
use crate::error::{Error, Result};
use crate::limits::{
    check_file_len, check_stack, clamp_name, FORMAT_VERSION, MAGIC, MAX_NAME_BYTES,
};

pub fn save_oxypix(doc: &Document, path: &Path) -> Result<()> {
    std::fs::write(path, save_oxypix_bytes(doc)?).map_err(Error::io)
}

pub fn load_oxypix(path: &Path) -> Result<Document> {
    let bytes = read_limited(path)?;
    load_oxypix_bytes(&bytes)
}

pub fn save_oxypix_bytes(doc: &Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
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
    Ok(out)
}

pub fn load_oxypix_bytes(bytes: &[u8]) -> Result<Document> {
    check_file_len(bytes.len() as u64)?;
    let mut r = Reader::new(bytes);
    let magic = r.bytes(8)?;
    if magic != MAGIC {
        return Err(Error::BadMagic);
    }
    let version = r.u16()?;
    if version != FORMAT_VERSION {
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
    if !r.finished() {
        return Err(Error::TrailingBytes);
    }
    Document::from_parts(
        width,
        height,
        layers,
        frames,
        active_frame,
        active_layer,
        palette,
    )
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

    #[test]
    fn file_size_cap_is_enforced_without_reading_pixels() {
        let err = check_file_len(MAX_FILE_BYTES + 1).unwrap_err();
        assert!(matches!(err, Error::Limit { .. }));
    }
}
