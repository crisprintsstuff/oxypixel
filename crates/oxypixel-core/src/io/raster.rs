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
    MAX_FRAMES, MAX_SHEET_PADDING, MAX_SHEET_PIXELS, MAX_SHEET_WIDTH,
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
    export_png_bytes_scaled(doc, frame, 1)
}

/// Write one composited frame as a PNG at 1×, 2×, or 4× nearest-neighbor scale.
pub fn export_png_scaled(doc: &Document, frame: usize, path: &Path, scale: u32) -> Result<()> {
    std::fs::write(path, export_png_bytes_scaled(doc, frame, scale)?).map_err(Error::io)
}

pub fn export_png_bytes_scaled(doc: &Document, frame: usize, scale: u32) -> Result<Vec<u8>> {
    let (width, height, pixels) = composite_scaled(doc, frame, scale)?;
    encode_still(&pack(&pixels), width, height, ImageFormat::Png)
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
    export_spritesheet_bytes_scaled(doc, 1)
}

/// Horizontal spritesheet at 1×, 2×, or 4× nearest-neighbor scale.
pub fn export_spritesheet_scaled(doc: &Document, path: &Path, scale: u32) -> Result<()> {
    std::fs::write(path, export_spritesheet_bytes_scaled(doc, scale)?).map_err(Error::io)
}

pub fn export_spritesheet_bytes_scaled(doc: &Document, scale: u32) -> Result<Vec<u8>> {
    export_spritesheet_layout_bytes(
        doc,
        SheetLayout {
            scale,
            ..SheetLayout::default()
        },
    )
}

/// How a spritesheet places its frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetLayout {
    /// 1, 2, or 4, nearest neighbor.
    pub scale: u32,
    /// Frames per row. 0, or more than the frame count, is one row.
    pub columns: u32,
    /// Clear pixels between neighboring cells, after scaling.
    pub padding: u32,
}

impl Default for SheetLayout {
    fn default() -> Self {
        Self {
            scale: 1,
            columns: 0,
            padding: 0,
        }
    }
}

/// Where each frame of a spritesheet sits. Frame `i` is `cells[i]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetGrid {
    pub width: u32,
    pub height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub cells: Vec<(u32, u32)>,
}

/// Lay out the frames row by row, left to right. Sizes past the caps are an error.
pub fn spritesheet_grid(doc: &Document, layout: SheetLayout) -> Result<SheetGrid> {
    if layout.padding > MAX_SHEET_PADDING {
        return Err(Error::Limit {
            what: "spritesheet padding",
            value: u64::from(layout.padding),
            max: u64::from(MAX_SHEET_PADDING),
        });
    }
    let frames = doc.frame_count() as u64;
    let cell_w = u64::from(checked_scale(doc.width(), layout.scale)?);
    let cell_h = u64::from(checked_scale(doc.height(), layout.scale)?);
    let columns = match u64::from(layout.columns) {
        0 => frames,
        n => n.min(frames),
    }
    .max(1);
    let rows = frames.div_ceil(columns).max(1);
    let pad = u64::from(layout.padding);
    // u64 throughout: every factor is capped well below 2^32.
    let width = columns * cell_w + (columns - 1) * pad;
    let height = rows * cell_h + (rows - 1) * pad;
    for (what, value) in [("spritesheet width", width), ("spritesheet height", height)] {
        if value > u64::from(MAX_SHEET_WIDTH) {
            return Err(Error::Limit {
                what,
                value,
                max: u64::from(MAX_SHEET_WIDTH),
            });
        }
    }
    if width * height > MAX_SHEET_PIXELS {
        return Err(Error::Limit {
            what: "spritesheet pixels",
            value: width * height,
            max: MAX_SHEET_PIXELS,
        });
    }
    let cells = (0..frames)
        .map(|i| {
            let (col, row) = (i % columns, i / columns);
            ((col * (cell_w + pad)) as u32, (row * (cell_h + pad)) as u32)
        })
        .collect();
    Ok(SheetGrid {
        width: width as u32,
        height: height as u32,
        cell_width: cell_w as u32,
        cell_height: cell_h as u32,
        cells,
    })
}

/// A spritesheet PNG with frames placed by `layout`.
pub fn export_spritesheet_layout_bytes(doc: &Document, layout: SheetLayout) -> Result<Vec<u8>> {
    let grid = spritesheet_grid(doc, layout)?;
    let sheet_w = grid.width as usize;
    let cell_w = grid.cell_width as usize;
    let mut out = vec![Rgba::CLEAR; sheet_w * grid.height as usize];
    for (index, &(x0, y0)) in grid.cells.iter().enumerate() {
        let (_, _, frame) = composite_scaled(doc, index, layout.scale)?;
        for (row, line) in frame.chunks_exact(cell_w).enumerate() {
            let start = (y0 as usize + row) * sheet_w + x0 as usize;
            out[start..start + cell_w].copy_from_slice(line);
        }
    }
    encode_still(&pack(&out), grid.width, grid.height, ImageFormat::Png)
}

/// The spritesheet description that game engines read as a JSON array
/// sheet: one entry per frame with its rectangle and duration, then `meta`.
pub fn spritesheet_json(doc: &Document, layout: SheetLayout, image: &str) -> Result<String> {
    let grid = spritesheet_grid(doc, layout)?;
    let (w, h) = (grid.cell_width, grid.cell_height);
    let mut out = String::from("{\n  \"frames\": [\n");
    for (index, (x, y)) in grid.cells.iter().enumerate() {
        let duration = doc.frame_duration(index).unwrap_or(DEFAULT_FRAME_MS);
        out.push_str(&format!(
            "    {{\"filename\": \"frame {index}\", \"frame\": {{\"x\": {x}, \"y\": {y}, \"w\": {w}, \"h\": {h}}}, \"rotated\": false, \"trimmed\": false, \"spriteSourceSize\": {{\"x\": 0, \"y\": 0, \"w\": {w}, \"h\": {h}}}, \"sourceSize\": {{\"w\": {w}, \"h\": {h}}}, \"duration\": {duration}}}"
        ));
        out.push_str(if index + 1 < grid.cells.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    out.push_str(&format!(
        "  ],\n  \"meta\": {{\"app\": \"OxyPixel\", \"version\": \"{}\", \"image\": {}, \"format\": \"RGBA8888\", \"size\": {{\"w\": {}, \"h\": {}}}, \"scale\": \"{}\", \"frameTags\": [{}]}}\n}}\n",
        env!("CARGO_PKG_VERSION"),
        json_string(image),
        grid.width,
        grid.height,
        layout.scale,
        frame_tags_json(doc.tags()),
    ));
    Ok(out)
}

/// Write the PNG, and `<same name>.json` beside it when `json` is set.
pub fn export_spritesheet_layout(
    doc: &Document,
    layout: SheetLayout,
    path: &Path,
    json: bool,
) -> Result<()> {
    let png = export_spritesheet_layout_bytes(doc, layout)?;
    let description = if json {
        let image = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Some(spritesheet_json(doc, layout, &image)?)
    } else {
        None
    };
    std::fs::write(path, png).map_err(Error::io)?;
    if let Some(text) = description {
        std::fs::write(path.with_extension("json"), text).map_err(Error::io)?;
    }
    Ok(())
}

/// The entries of a JSON sheet's `meta.frameTags`, as Aseprite writes them.
/// `repeat` is written only when it is not 0.
pub fn frame_tags_json(tags: &[crate::tags::FrameTag]) -> String {
    tags.iter()
        .map(|tag| {
            let repeat = if tag.repeat == 0 {
                String::new()
            } else {
                format!(", \"repeat\": \"{}\"", tag.repeat)
            };
            format!(
                "{{\"name\": {}, \"from\": {}, \"to\": {}, \"direction\": \"{}\"{repeat}}}",
                json_string(&tag.name),
                tag.from,
                tag.to,
                tag.direction.json_name(),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

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

fn checked_scale(side: u32, scale: u32) -> Result<u32> {
    if !matches!(scale, 1 | 2 | 4) {
        return Err(Error::Limit {
            what: "export scale",
            value: u64::from(scale),
            max: 4,
        });
    }
    side.checked_mul(scale).ok_or(Error::Limit {
        what: "export size",
        value: u64::MAX,
        max: u64::from(u32::MAX),
    })
}

fn composite_scaled(doc: &Document, frame: usize, scale: u32) -> Result<(u32, u32, Vec<Rgba>)> {
    let width = checked_scale(doc.width(), scale)?;
    let height = checked_scale(doc.height(), scale)?;
    let src = doc.composite(frame)?;
    Ok((
        width,
        height,
        scale_nearest(&src, doc.width(), doc.height(), scale)?,
    ))
}

/// Repeat each pixel into a `scale` by `scale` block. `scale` is 1, 2, or 4.
pub fn scale_nearest(pixels: &[Rgba], width: u32, height: u32, scale: u32) -> Result<Vec<Rgba>> {
    let dst_w = checked_scale(width, scale)?;
    let dst_h = checked_scale(height, scale)?;
    if pixels.len() != (width as usize) * (height as usize) {
        return Err(Error::Truncated);
    }
    if scale == 1 {
        return Ok(pixels.to_vec());
    }
    let mut out = vec![Rgba::CLEAR; (dst_w as usize) * (dst_h as usize)];
    for y in 0..height {
        for x in 0..width {
            let px = pixels[(y * width + x) as usize];
            for dy in 0..scale {
                for dx in 0..scale {
                    let nx = x * scale + dx;
                    let ny = y * scale + dy;
                    out[(ny * dst_w + nx) as usize] = px;
                }
            }
        }
    }
    Ok(out)
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

pub(crate) fn encode_rgba_png(width: u32, height: u32, pixels: &[Rgba]) -> Result<Vec<u8>> {
    check_canvas(width, height)?;
    let count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(Error::Truncated)?;
    if pixels.len() != count {
        return Err(Error::Truncated);
    }
    encode_still(&pack(pixels), width, height, ImageFormat::Png)
}

pub(crate) fn decode_png_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<Rgba>)> {
    check_len(bytes)?;
    if guess(bytes)? != ImageFormat::Png {
        return Err(Error::UnsupportedFormat);
    }
    decode_still(bytes)
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

    fn five_frames() -> Document {
        let mut doc = Document::new(2, 1).unwrap();
        for i in 0..5u8 {
            if i > 0 {
                doc.add_frame().unwrap();
            }
            let frame = &mut doc.frames_mut()[i as usize];
            frame.duration_ms = 100 + u32::from(i) * 10;
            frame.cels[0][0] = Rgba::new(10 + i, 0, 0, 255);
        }
        doc
    }

    #[test]
    fn a_sheet_grid_wraps_rows_and_adds_padding() {
        let doc = five_frames();
        let row = spritesheet_grid(&doc, SheetLayout::default()).unwrap();
        assert_eq!((row.width, row.height), (10, 1));
        assert_eq!(row.cells[4], (8, 0));

        let layout = SheetLayout {
            scale: 2,
            columns: 2,
            padding: 3,
        };
        let grid = spritesheet_grid(&doc, layout).unwrap();
        // Two columns of 4-pixel cells with a 3-pixel gap; three rows of 2.
        assert_eq!((grid.cell_width, grid.cell_height), (4, 2));
        assert_eq!((grid.width, grid.height), (11, 12));
        assert_eq!(grid.cells, vec![(0, 0), (7, 0), (0, 5), (7, 5), (0, 10)]);

        let bytes = export_spritesheet_layout_bytes(&doc, layout).unwrap();
        let (w, h, pixels) = decode_first_rgba(&bytes).unwrap();
        assert_eq!((w, h), (11, 12));
        let at = |x: usize, y: usize| pixels[y * 11 + x];
        assert_eq!(at(0, 0).r, 10);
        assert_eq!(at(1, 1).r, 10);
        assert_eq!(at(7, 0).r, 11);
        assert_eq!(at(0, 10).r, 14);
        assert_eq!(at(5, 0), Rgba::CLEAR);
        assert_eq!(at(0, 3), Rgba::CLEAR);
        assert_eq!(at(10, 11), Rgba::CLEAR);

        // More columns than frames is one row; the one-row bytes are unchanged.
        let wide = SheetLayout {
            columns: 99,
            ..SheetLayout::default()
        };
        assert_eq!(
            export_spritesheet_layout_bytes(&doc, wide).unwrap(),
            export_spritesheet_bytes(&doc).unwrap()
        );
    }

    #[test]
    fn a_sheet_past_the_caps_is_an_error() {
        let doc = Document::new(1024, 1024).unwrap();
        let mut long = doc.clone();
        for _ in 0..15 {
            long.add_frame().unwrap();
        }
        let tall = SheetLayout {
            scale: 4,
            columns: 1,
            padding: 0,
        };
        assert!(matches!(
            spritesheet_grid(&long, tall),
            Err(Error::Limit {
                what: "spritesheet height",
                ..
            })
        ));
        let square = SheetLayout {
            scale: 4,
            columns: 4,
            padding: 0,
        };
        assert!(matches!(
            spritesheet_grid(&long, square),
            Err(Error::Limit {
                what: "spritesheet pixels",
                ..
            })
        ));
        let padded = SheetLayout {
            padding: MAX_SHEET_PADDING + 1,
            ..SheetLayout::default()
        };
        assert!(spritesheet_grid(&doc, padded).is_err());
        assert!(spritesheet_grid(
            &doc,
            SheetLayout {
                scale: 3,
                ..SheetLayout::default()
            }
        )
        .is_err());
    }

    #[test]
    fn the_sheet_json_lists_each_frame_rectangle_and_duration() {
        let doc = five_frames();
        let layout = SheetLayout {
            scale: 1,
            columns: 2,
            padding: 1,
        };
        let text = spritesheet_json(&doc, layout, "walk \"cycle\".png").unwrap();
        assert!(text.starts_with("{\n  \"frames\": [\n"));
        assert_eq!(text.matches("\"filename\"").count(), 5);
        assert!(text.contains(
            "\"filename\": \"frame 1\", \"frame\": {\"x\": 3, \"y\": 0, \"w\": 2, \"h\": 1}"
        ));
        assert!(text.contains("\"frame\": {\"x\": 0, \"y\": 4, \"w\": 2, \"h\": 1}"));
        assert!(text.contains("\"duration\": 140}\n"));
        assert!(text.contains("\"image\": \"walk \\\"cycle\\\".png\""));
        assert!(text.contains("\"size\": {\"w\": 5, \"h\": 5}"));
        assert!(text.contains("\"frameTags\": []"));
        assert_eq!(text.matches('{').count(), text.matches('}').count());

        let mut tagged = doc.clone();
        tagged
            .add_tag(crate::tags::FrameTag {
                name: "run".into(),
                from: 1,
                to: 3,
                direction: crate::tags::LoopDirection::PingPong,
                repeat: 2,
            })
            .unwrap();
        let text = spritesheet_json(&tagged, layout, "walk.png").unwrap();
        assert!(text.contains(
            "\"frameTags\": [{\"name\": \"run\", \"from\": 1, \"to\": 3, \"direction\": \"pingpong\", \"repeat\": \"2\"}]"
        ), "{text}");
    }

    #[test]
    fn nearest_export_keeps_hard_pixel_blocks() {
        let doc = painted();
        let bytes = export_png_bytes_scaled(&doc, 0, 2).unwrap();
        let (w, h, pixels) = decode_first_rgba(&bytes).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(pixels[0], Rgba::new(255, 0, 0, 255));
        assert_eq!(pixels[1], pixels[0]);
        assert_eq!(pixels[2].b, 255);
        assert!(export_png_bytes_scaled(&doc, 0, 3).is_err());
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
