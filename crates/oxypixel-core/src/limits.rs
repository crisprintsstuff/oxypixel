//! Hard limits for canvases, animations, and files.
//!
//! The caps keep a corrupt or hostile file from allocating an unbounded buffer.
//! They also keep undo snapshots inside a fixed memory budget.

use crate::error::{Error, Result};

/// Largest width or height, in pixels.
pub const MAX_DIMENSION: u32 = 1024;
/// Largest single cel (`width * height`).
pub const MAX_CEL_PIXELS: u64 = 1024 * 1024;
/// Largest `width * height * frames * layers` product.
pub const MAX_TOTAL_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_FRAMES: usize = 256;
pub const MAX_LAYERS: usize = 32;
pub const MAX_PALETTE: usize = 256;
pub const MAX_NAME_BYTES: usize = 128;
/// Reject files larger than this before parsing or decoding.
pub const MAX_FILE_BYTES: u64 = 80 * 1024 * 1024;
pub const MIN_FRAME_MS: u32 = 20;
pub const MAX_FRAME_MS: u32 = 10_000;
pub const DEFAULT_FRAME_MS: u32 = 100;
pub const MAX_BRUSH: u32 = 16;
pub const MAX_UNDO: usize = 50;
pub const MAX_UNDO_BYTES: usize = 256 * 1024 * 1024;
pub const FORMAT_VERSION: u16 = 1;
pub const MAGIC: &[u8; 8] = b"OXYPIX1\0";
/// Widest spritesheet written by export.
pub const MAX_SHEET_WIDTH: u32 = 16_384;

pub(crate) fn check_canvas(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(Error::Dimensions {
            width,
            height,
            max: MAX_DIMENSION,
        });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_CEL_PIXELS {
        return Err(Error::Limit {
            what: "pixels in one cel",
            value: pixels,
            max: MAX_CEL_PIXELS,
        });
    }
    Ok(())
}

pub(crate) fn check_stack(width: u32, height: u32, frames: usize, layers: usize) -> Result<()> {
    check_canvas(width, height)?;
    if frames == 0 || frames > MAX_FRAMES {
        return Err(Error::Limit {
            what: "frames",
            value: frames as u64,
            max: MAX_FRAMES as u64,
        });
    }
    if layers == 0 || layers > MAX_LAYERS {
        return Err(Error::Limit {
            what: "layers",
            value: layers as u64,
            max: MAX_LAYERS as u64,
        });
    }
    let total = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|n| n.checked_mul(frames as u64))
        .and_then(|n| n.checked_mul(layers as u64))
        .ok_or(Error::Limit {
            what: "total pixels",
            value: u64::MAX,
            max: MAX_TOTAL_PIXELS,
        })?;
    if total > MAX_TOTAL_PIXELS {
        return Err(Error::Limit {
            what: "total pixels",
            value: total,
            max: MAX_TOTAL_PIXELS,
        });
    }
    Ok(())
}

pub(crate) fn check_file_len(len: u64) -> Result<()> {
    if len > MAX_FILE_BYTES {
        return Err(Error::Limit {
            what: "file bytes",
            value: len,
            max: MAX_FILE_BYTES,
        });
    }
    Ok(())
}

pub(crate) fn clamp_duration(ms: u32) -> u32 {
    ms.clamp(MIN_FRAME_MS, MAX_FRAME_MS)
}

pub(crate) fn clamp_name(name: &str) -> String {
    let mut end = name.len().min(MAX_NAME_BYTES);
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    #[test]
    fn pixel_budget_fails_before_any_canvas_exists() {
        let err = check_stack(1024, 1024, 32, 2).unwrap_err();
        assert!(matches!(err, Error::Limit { .. }));
        assert!(check_stack(64, 64, 8, 4).is_ok());
    }
}
