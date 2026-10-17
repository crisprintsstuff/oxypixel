//! PNG, BMP, and GIF import/export.
//!
//! GIF import uses the `image` crate's decoder, which composites disposal onto
//! full frames. Export writes the composited frames and loops forever.

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use image::codecs::bmp::BmpEncoder;
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::png::PngEncoder;
use image::{
    AnimationDecoder, Delay, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat,
    ImageReader, Limits, RgbaImage,
};

use crate::color::{pack, unpack, Rgba};
use crate::document::{Document, Frame, LayerInfo};
use crate::error::{Error, Result};
use crate::limits::{
    check_canvas, check_stack, clamp_duration, DEFAULT_FRAME_MS, MAX_DIMENSION, MAX_FILE_BYTES,
    MAX_FRAMES, MAX_SHEET_WIDTH,
};

use super::oxypix::read_limited;

/// How an imported bitmap was fitted onto a canvas of a different size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Exact,
    Cropped,
    Padded,
    CroppedAndPadded,
}

pub fn import_image(path: &Path) -> Result<Document> {
    import_image_bytes(&read_limited(path)?)
}

pub fn import_image_bytes(bytes: &[u8]) -> Result<Document> {
    check_len(bytes)?;
    match guess(bytes)? {
        ImageFormat::Gif => import_gif(bytes),
        ImageFormat::Png | ImageFormat::Bmp => {
            let (w, h, pixels) = decode_still(bytes)?;
            still_document(w, h, pixels)
        }
        _ => Err(Error::UnsupportedFormat),
    }
}

/// First frame of a PNG, BMP, or GIF as RGBA.
pub fn decode_first_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<Rgba>)> {
    check_len(bytes)?;
    match guess(bytes)? {
        ImageFormat::Gif => {
            let doc = import_gif(bytes)?;
            let pixels = doc.cel(0, 0)?.to_vec();
            Ok((doc.width(), doc.height(), pixels))
        }
        ImageFormat::Png | ImageFormat::Bmp => decode_still(bytes),
        _ => Err(Error::UnsupportedFormat),
    }
}

/// Copy `src` into a buffer of `dst_w` by `dst_h`. Extra source pixels are
/// cropped. Missing pixels stay transparent.
pub fn fit_into(
    src_w: u32,
    src_h: u32,
    src: &[Rgba],
    dst_w: u32,
    dst_h: u32,
) -> Result<(Vec<Rgba>, Fit)> {
    check_canvas(dst_w, dst_h)?;
    if src.len() != (src_w as usize) * (src_h as usize) {
        return Err(Error::Truncated);
    }
    let mut out = vec![Rgba::CLEAR; (dst_w as usize) * (dst_h as usize)];
    let copy_w = src_w.min(dst_w);
    let copy_h = src_h.min(dst_h);
    for y in 0..copy_h {
        for x in 0..copy_w {
            let s = (y * src_w + x) as usize;
            let d = (y * dst_w + x) as usize;
            out[d] = src[s];
        }
    }
    let wider = src_w > dst_w || src_h > dst_h;
    let smaller = src_w < dst_w || src_h < dst_h;
    let fit = match (wider, smaller) {
        (false, false) => Fit::Exact,
        (true, false) => Fit::Cropped,
        (false, true) => Fit::Padded,
        (true, true) => Fit::CroppedAndPadded,
    };
    Ok((out, fit))
}

pub fn export_png(doc: &Document, frame: usize, path: &Path) -> Result<()> {
    std::fs::write(path, export_png_bytes(doc, frame)?).map_err(Error::io)
}

pub fn export_png_bytes(doc: &Document, frame: usize) -> Result<Vec<u8>> {
    let pixels = doc.composite(frame)?;
    encode_still(&pack(&pixels), doc.width(), doc.height(), ImageFormat::Png)
}

pub fn export_bmp(doc: &Document, frame: usize, path: &Path) -> Result<()> {
    std::fs::write(path, export_bmp_bytes(doc, frame)?).map_err(Error::io)
}

pub fn export_bmp_bytes(doc: &Document, frame: usize) -> Result<Vec<u8>> {
    let pixels = doc.composite(frame)?;
    encode_still(&pack(&pixels), doc.width(), doc.height(), ImageFormat::Bmp)
}

pub fn export_gif(doc: &Document, path: &Path) -> Result<()> {
    std::fs::write(path, export_gif_bytes(doc)?).map_err(Error::io)
}

pub fn export_gif_bytes(doc: &Document) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    let mut encoder = GifEncoder::new_with_speed(&mut cursor, 10);
    encoder.set_repeat(Repeat::Infinite).map_err(Error::image)?;
    for index in 0..doc.frame_count() {
        let pixels = pack(&doc.composite(index)?);
        let image =
            RgbaImage::from_raw(doc.width(), doc.height(), pixels).ok_or(Error::Truncated)?;
        let ms = doc.frame_duration(index).unwrap_or(DEFAULT_FRAME_MS);
        let frame = image::Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(ms, 1));
        encoder.encode_frame(frame).map_err(Error::image)?;
    }
    drop(encoder);
    Ok(cursor.into_inner())
}

pub fn export_spritesheet(doc: &Document, path: &Path) -> Result<()> {
    std::fs::write(path, export_spritesheet_bytes(doc)?).map_err(Error::io)
}

pub fn export_spritesheet_bytes(doc: &Document) -> Result<Vec<u8>> {
    let frames = doc.frame_count() as u32;
    let sheet_w = doc.width().checked_mul(frames).ok_or(Error::Limit {
        what: "spritesheet width",
        value: u64::MAX,
        max: u64::from(MAX_SHEET_WIDTH),
    })?;
    if sheet_w > MAX_SHEET_WIDTH {
        return Err(Error::Limit {
            what: "spritesheet width",
            value: u64::from(sheet_w),
            max: u64::from(MAX_SHEET_WIDTH),
        });
    }
    let sheet_h = doc.height();
    let mut out = vec![0u8; (sheet_w as usize) * (sheet_h as usize) * 4];
    for index in 0..doc.frame_count() {
        let composite = pack(&doc.composite(index)?);
        let x0 = index * doc.width() as usize;
        for y in 0..sheet_h as usize {
            let src = y * doc.width() as usize * 4;
            let dst = (y * sheet_w as usize + x0) * 4;
            let n = doc.width() as usize * 4;
            out[dst..dst + n].copy_from_slice(&composite[src..src + n]);
        }
    }
    encode_still(&out, sheet_w, sheet_h, ImageFormat::Png)
}

fn still_document(width: u32, height: u32, pixels: Vec<Rgba>) -> Result<Document> {
    Document::from_parts(
        width,
        height,
        vec![LayerInfo {
            name: "Imported".to_string(),
            visible: true,
            opacity: 255,
        }],
        vec![Frame {
            duration_ms: DEFAULT_FRAME_MS,
            cels: vec![pixels],
        }],
        0,
        0,
        crate::document::default_palette(),
    )
}

fn import_gif(bytes: &[u8]) -> Result<Document> {
    let mut decoder = GifDecoder::new(Cursor::new(bytes)).map_err(Error::image)?;
    let (width, height) = decoder.dimensions();
    check_canvas(width, height)?;
    decoder.set_limits(limits()).map_err(Error::image)?;
    let decoded = decoder
        .into_frames()
        .collect_frames()
        .map_err(Error::image)?;
    if decoded.is_empty() {
        return Err(Error::Truncated);
    }
    if decoded.len() > MAX_FRAMES {
        return Err(Error::Limit {
            what: "frames",
            value: decoded.len() as u64,
            max: MAX_FRAMES as u64,
        });
    }
    check_stack(width, height, decoded.len(), 1)?;
    let mut frames = Vec::with_capacity(decoded.len());
    for frame in decoded {
        if frame.buffer().width() != width || frame.buffer().height() != height {
            return Err(Error::Truncated);
        }
        let pixels = unpack(frame.buffer().as_raw()).ok_or(Error::Truncated)?;
        let ms = Duration::from(frame.delay()).as_millis();
        let duration_ms = if ms == 0 {
            DEFAULT_FRAME_MS
        } else {
            clamp_duration(u32::try_from(ms).unwrap_or(DEFAULT_FRAME_MS))
        };
        frames.push(Frame {
            duration_ms,
            cels: vec![pixels],
        });
    }
    Document::from_parts(
        width,
        height,
        vec![LayerInfo {
            name: "Imported".to_string(),
            visible: true,
            opacity: 255,
        }],
        frames,
        0,
        0,
        crate::document::default_palette(),
    )
}

fn decode_still(bytes: &[u8]) -> Result<(u32, u32, Vec<Rgba>)> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(Error::image)?;
    reader.limits(limits());
    let image = reader.decode().map_err(Error::image)?.to_rgba8();
    let (width, height) = image.dimensions();
    check_canvas(width, height)?;
    let pixels = unpack(image.as_raw()).ok_or(Error::Truncated)?;
    Ok((width, height, pixels))
}

fn encode_still(bytes: &[u8], width: u32, height: u32, format: ImageFormat) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    match format {
        ImageFormat::Png => PngEncoder::new(&mut cursor)
            .write_image(bytes, width, height, ExtendedColorType::Rgba8)
            .map_err(Error::image)?,
        ImageFormat::Bmp => BmpEncoder::new(&mut cursor)
            .write_image(bytes, width, height, ExtendedColorType::Rgba8)
            .map_err(Error::image)?,
        _ => return Err(Error::UnsupportedFormat),
    }
    Ok(cursor.into_inner())
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits
}

fn guess(bytes: &[u8]) -> Result<ImageFormat> {
    image::guess_format(bytes).map_err(|_| Error::UnsupportedFormat)
}

fn check_len(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Limit {
            what: "file bytes",
            value: bytes.len() as u64,
            max: MAX_FILE_BYTES,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted() -> Document {
        let mut doc = Document::new(2, 1).unwrap();
        doc.frames_mut()[0].cels[0][0] = Rgba::new(255, 0, 0, 255);
        doc.frames_mut()[0].cels[0][1] = Rgba::new(0, 0, 255, 255);
        doc
    }

    #[test]
    fn png_roundtrip_is_exact() {
        let doc = painted();
        let bytes = export_png_bytes(&doc, 0).unwrap();
        let loaded = import_image_bytes(&bytes).unwrap();
        assert_eq!(loaded.width(), 2);
        assert_eq!(loaded.cel(0, 0).unwrap(), doc.cel(0, 0).unwrap());
    }

    #[test]
    fn bmp_roundtrip_keeps_opaque_color() {
        let doc = painted();
        let bytes = export_bmp_bytes(&doc, 0).unwrap();
        let loaded = import_image_bytes(&bytes).unwrap();
        let px = loaded.cel(0, 0).unwrap()[0];
        assert_eq!((px.r, px.g, px.b), (255, 0, 0));
        assert_eq!(px.a, 255);
    }

    #[test]
    fn gif_roundtrip_keeps_frame_count_and_solids() {
        let mut doc = Document::new(2, 2).unwrap();
        for px in &mut doc.frames_mut()[0].cels[0] {
            *px = Rgba::new(255, 0, 0, 255);
        }
        doc.add_frame().unwrap();
        for px in &mut doc.frames_mut()[1].cels[0] {
            *px = Rgba::new(0, 0, 255, 255);
        }
        doc.set_frame_duration(0, 100).unwrap();
        doc.set_frame_duration(1, 100).unwrap();
        let bytes = export_gif_bytes(&doc).unwrap();
        let loaded = import_image_bytes(&bytes).unwrap();
        assert_eq!(loaded.frame_count(), 2);
        assert_eq!(loaded.cel(0, 0).unwrap()[0].r, 255);
        assert_eq!(loaded.cel(1, 0).unwrap()[0].b, 255);
    }

    #[test]
    fn spritesheet_places_frames_side_by_side() {
        let mut doc = Document::new(1, 1).unwrap();
        doc.frames_mut()[0].cels[0][0] = Rgba::new(1, 0, 0, 255);
        doc.add_frame().unwrap();
        doc.frames_mut()[1].cels[0][0] = Rgba::new(0, 1, 0, 255);
        let bytes = export_spritesheet_bytes(&doc).unwrap();
        let (w, h, pixels) = decode_first_rgba(&bytes).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(pixels[0].r, 1);
        assert_eq!(pixels[1].g, 1);
    }

    #[test]
    fn fit_reports_crop_and_pad() {
        let src = vec![Rgba::new(1, 0, 0, 255), Rgba::new(2, 0, 0, 255)];
        let (out, fit) = fit_into(2, 1, &src, 1, 1).unwrap();
        assert_eq!(fit, Fit::Cropped);
        assert_eq!(out[0].r, 1);
        let (_, fit) = fit_into(1, 1, &src[..1], 2, 1).unwrap();
        assert_eq!(fit, Fit::Padded);
    }

    #[test]
    fn rejects_non_images() {
        assert!(matches!(
            import_image_bytes(b"hello"),
            Err(Error::UnsupportedFormat)
        ));
    }
}
