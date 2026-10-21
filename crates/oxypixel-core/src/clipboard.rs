//! Straight-alpha images for the selection clipboard.
//!
//! The editor stores a pixel patch in memory. These functions turn that patch
//! into PNG or raw RGBA8 and back. They do not open an operating-system
//! clipboard. The `oxypixel` window does that, and falls back to the patch
//! when the system clipboard is missing.

use crate::color::{pack, unpack, Rgba};
use crate::error::{Error, Result};
use crate::io::{decode_png_rgba, encode_rgba_png};
use crate::limits::check_canvas;

/// Byte length of a straight-alpha RGBA image that fits the canvas caps.
fn rgba_len(width: u32, height: u32) -> Result<usize> {
    check_canvas(width, height)?;
    let pixels = u64::from(width) * u64::from(height);
    usize::try_from(pixels * 4).map_err(|_| Error::Truncated)
}

/// Pack pixels as row-major RGBA8. Channels stay straight, including alpha 0.
pub(crate) fn pixels_to_rgba(pixels: &[Rgba]) -> Vec<u8> {
    pack(pixels)
}

/// Unpack row-major RGBA8. The size is rejected before a pixel buffer is allocated.
pub(crate) fn pixels_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<Rgba>> {
    let expected = rgba_len(width, height)?;
    if rgba.len() != expected {
        return Err(Error::Truncated);
    }
    unpack(rgba).ok_or(Error::Truncated)
}

pub(crate) fn encode_png(width: u32, height: u32, pixels: &[Rgba]) -> Result<Vec<u8>> {
    encode_rgba_png(width, height, pixels)
}

pub(crate) fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<Rgba>)> {
    decode_png_rgba(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Editor, MouseButton};
    use crate::limits::MAX_DIMENSION;

    fn sample() -> Vec<Rgba> {
        vec![
            Rgba::new(9, 8, 7, 0),
            Rgba::new(1, 2, 3, 128),
            Rgba::new(4, 5, 6, 255),
            Rgba::CLEAR,
        ]
    }

    #[test]
    fn png_and_rgba_keep_straight_alpha() {
        let pixels = sample();
        let png = encode_png(2, 2, &pixels).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let (width, height, decoded) = decode_png(&png).unwrap();
        assert_eq!((width, height), (2, 2));
        assert_eq!(decoded, pixels);

        let rgba = pixels_to_rgba(&pixels);
        assert_eq!(rgba.len(), 16);
        assert_eq!(pixels_from_rgba(2, 2, &rgba).unwrap(), pixels);
    }

    #[test]
    fn hostile_sizes_fail_without_allocating_the_claimed_buffer() {
        assert!(pixels_from_rgba(0, 1, &[]).is_err());
        assert!(pixels_from_rgba(MAX_DIMENSION + 1, 1, &[0, 0, 0, 255]).is_err());
        assert!(pixels_from_rgba(u32::MAX, u32::MAX, &[0, 0, 0, 255]).is_err());
        assert!(pixels_from_rgba(1, 1, &[0, 0, 0]).is_err());
        assert!(pixels_from_rgba(1, 1, &[0, 0, 0, 255, 9]).is_err());
        assert!(encode_png(1, 1, &[]).is_err());
        assert!(decode_png(&[]).is_err());
        assert!(decode_png(b"not a png").is_err());

        // IHDR claims 8000×8000, which is outside the canvas cap. The CRC is
        // intentionally wrong. Either failure is fine; it must not panic.
        let mut header = b"\x89PNG\r\n\x1a\n".to_vec();
        header.extend_from_slice(&13u32.to_be_bytes());
        header.extend_from_slice(b"IHDR");
        header.extend_from_slice(&8_000u32.to_be_bytes());
        header.extend_from_slice(&8_000u32.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        header.extend_from_slice(&0u32.to_be_bytes());
        assert!(decode_png(&header).is_err());
    }

    #[test]
    fn selection_roundtrip_keeps_alpha_and_a_bad_image_keeps_the_patch() {
        let mut ed = Editor::new(2, 2).unwrap();
        ed.set_primary(Rgba::new(9, 8, 7, 0));
        ed.pointer_down(0, 0, MouseButton::Primary, false);
        ed.pointer_up(0, 0, false);
        ed.set_primary(Rgba::new(1, 2, 3, 128));
        ed.pointer_down(1, 0, MouseButton::Primary, false);
        ed.pointer_up(1, 0, false);
        ed.select_all();
        assert!(ed.copy_selection());

        let (width, height, rgba) = ed.clipboard_rgba().unwrap();
        assert_eq!((width, height), (2, 2));
        let png = ed.clipboard_png().unwrap();

        assert!(ed.set_clipboard_rgba(0, 1, &[]).is_err());
        assert!(ed
            .set_clipboard_rgba(u32::MAX, u32::MAX, &[0, 0, 0, 255])
            .is_err());
        assert!(ed.set_clipboard_rgba(1, 1, &[1, 2, 3]).is_err());
        assert!(ed.set_clipboard_png(b"nope").is_err());
        assert!(ed.set_clipboard_png(&[]).is_err());
        assert_eq!(ed.clipboard_rgba().unwrap().2, rgba);

        let mut from_rgba = Editor::new(2, 2).unwrap();
        from_rgba.set_clipboard_rgba(width, height, &rgba).unwrap();
        from_rgba.paste_clipboard().unwrap();
        let mut from_png = Editor::new(2, 2).unwrap();
        from_png.set_clipboard_png(&png).unwrap();
        from_png.paste_clipboard().unwrap();
        let painted = from_rgba.document().cel(0, 0).unwrap();
        assert_eq!(painted[0], Rgba::new(9, 8, 7, 0));
        assert_eq!(painted[1], Rgba::new(1, 2, 3, 128));
        assert_eq!(painted[2], Rgba::CLEAR);
        assert_eq!(from_png.document().cel(0, 0).unwrap(), painted);

        let fresh = Editor::new(2, 2).unwrap();
        assert!(matches!(fresh.clipboard_png(), Err(Error::EmptyClipboard)));
        assert!(matches!(fresh.clipboard_rgba(), Err(Error::EmptyClipboard)));
    }
}
