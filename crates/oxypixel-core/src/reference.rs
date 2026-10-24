//! Reference images sit beside the document. They are not layers.
//!
//! A version 1 `.oxypix` file rejects trailing bytes, so a reference cannot be
//! stored in the document without a new format version. Paths and placement
//! live in a sidecar named `<file>.oxypix.refs`. The pixels stay in the PNG or
//! JPEG that path points at. [`crate::Document::composite`], export, and merge
//! never read this type.

use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegDecoder;
use image::codecs::png::PngDecoder;
use image::{DynamicImage, ImageDecoder, ImageFormat, Limits};

use crate::color::Rgba;
use crate::error::{Error, Result};
use crate::limits::{check_file_len, MAX_FILE_BYTES};

/// How many reference images one document may keep.
pub const MAX_REFERENCES: usize = 8;
/// Longest side of a decoded reference, in pixels.
pub const MAX_REFERENCE_SIDE: u32 = 2048;
const MAX_SIDECAR_BYTES: u64 = 256 * 1024;
const MAX_PATH_BYTES: usize = 4096;
/// Smallest overlay scale. A 2048-pixel reference still shrinks to 32 pixels.
pub const MIN_REFERENCE_SCALE: f32 = 1.0 / 64.0;
/// Largest overlay scale.
pub const MAX_REFERENCE_SCALE: f32 = 8.0;
const MAX_OFFSET: f32 = 100_000.0;
const MAGIC: &str = "OXYREFS1";

/// One decoded reference and the placement the window draws.
#[derive(Clone, Debug)]
struct Image {
    id: u64,
    path: PathBuf,
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
    missing: bool,
    visible: bool,
    opacity: u8,
    locked: bool,
    /// Drawn under the art. False draws it on top.
    behind: bool,
    /// Shown in a floating panel instead of on the canvas.
    panel: bool,
    /// The eyedropper may read this image. Export still skips it.
    sample: bool,
    x: f32,
    y: f32,
    scale: f32,
}

/// The reference list for one open document. Not part of the pixel stack.
#[derive(Clone, Debug)]
pub struct References {
    items: Vec<Image>,
    active: Option<usize>,
    next_id: u64,
    revision: u64,
}

/// What the window needs to draw one overlay without borrowing the pixel buffer twice.
pub struct Overlay<'a> {
    pub id: u64,
    pub pixels: &'a [Rgba],
    pub width: u32,
    pub height: u32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: u8,
    pub behind: bool,
    pub active: bool,
}

impl Default for References {
    fn default() -> Self {
        Self::new()
    }
}

impl References {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            active: None,
            next_id: 1,
            revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn set_active(&mut self, index: Option<usize>) {
        self.active = index.filter(|index| *index < self.items.len());
    }

    /// Decode a PNG or JPEG and append it. The document pixels are not touched.
    pub fn add_file(&mut self, path: &Path) -> Result<()> {
        if self.items.len() >= MAX_REFERENCES {
            return Err(Error::Limit {
                what: "references",
                value: (self.items.len() + 1) as u64,
                max: MAX_REFERENCES as u64,
            });
        }
        let image = self.decode_file(path)?;
        self.items.push(image);
        self.active = Some(self.items.len() - 1);
        self.touch();
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> Result<()> {
        if index >= self.items.len() {
            return Err(Error::Index { what: "reference" });
        }
        self.items.remove(index);
        self.active = match self.active {
            Some(active) if active == index => self.items.first().map(|_| 0),
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        self.touch();
        Ok(())
    }

    pub fn name(&self, index: usize) -> Option<String> {
        let image = self.items.get(index)?;
        let name = image
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "reference".to_string());
        if image.missing {
            Some(format!("{name} (missing)"))
        } else {
            Some(name)
        }
    }

    pub fn missing(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|image| image.missing)
    }

    pub fn visible(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|image| image.visible)
    }

    pub fn opacity(&self, index: usize) -> u8 {
        self.items
            .get(index)
            .map(|image| image.opacity)
            .unwrap_or(0)
    }

    pub fn locked(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|image| image.locked)
    }

    pub fn behind(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|image| image.behind)
    }

    pub fn panel(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|image| image.panel)
    }

    pub fn samples(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|image| image.sample)
    }

    pub fn scale(&self, index: usize) -> f32 {
        self.items
            .get(index)
            .map(|image| image.scale)
            .unwrap_or(1.0)
    }

    pub fn placement(&self, index: usize) -> Option<(f32, f32)> {
        self.items.get(index).map(|image| (image.x, image.y))
    }

    pub fn set_visible(&mut self, index: usize, visible: bool) -> Result<()> {
        self.edit(index, |image| image.visible = visible)
    }

    pub fn set_opacity(&mut self, index: usize, opacity: u8) -> Result<()> {
        self.edit(index, |image| image.opacity = opacity)
    }

    pub fn set_locked(&mut self, index: usize, locked: bool) -> Result<()> {
        self.edit(index, |image| image.locked = locked)
    }

    pub fn set_behind(&mut self, index: usize, behind: bool) -> Result<()> {
        self.edit(index, |image| image.behind = behind)
    }

    pub fn set_panel(&mut self, index: usize, panel: bool) -> Result<()> {
        self.edit(index, |image| image.panel = panel)
    }

    pub fn set_sample(&mut self, index: usize, sample: bool) -> Result<()> {
        self.edit(index, |image| image.sample = sample)
    }

    pub fn set_scale(&mut self, index: usize, scale: f32) -> Result<()> {
        if !scale.is_finite() {
            return Err(Error::BadReference);
        }
        let scale = scale.clamp(MIN_REFERENCE_SCALE, MAX_REFERENCE_SCALE);
        self.edit(index, |image| image.scale = scale)
    }

    /// Width and height on the canvas, in document pixels, at the current scale.
    pub fn display_size(&self, index: usize) -> Option<(f32, f32)> {
        let image = self.items.get(index)?;
        if image.missing || image.width == 0 || image.height == 0 {
            return None;
        }
        Some((
            image.width as f32 * image.scale,
            image.height as f32 * image.scale,
        ))
    }

    /// Scale so the image is `width` document pixels wide. The height follows.
    pub fn set_display_width(&mut self, index: usize, width: f32) -> Result<()> {
        let (current, _) = self.display_size(index).ok_or(Error::BadReference)?;
        self.set_display_side(index, width, current)
    }

    /// Scale so the image is `height` document pixels tall. The width follows.
    pub fn set_display_height(&mut self, index: usize, height: f32) -> Result<()> {
        let (_, current) = self.display_size(index).ok_or(Error::BadReference)?;
        self.set_display_side(index, height, current)
    }

    fn set_display_side(&mut self, index: usize, wanted: f32, current: f32) -> Result<()> {
        if !wanted.is_finite() || wanted <= 0.0 || current <= 0.0 {
            return Err(Error::BadReference);
        }
        let scale = self.scale(index) * wanted / current;
        self.set_scale(index, scale)
    }

    /// Scale the image to the largest size that fits the canvas, and center it.
    ///
    /// The scale stays within the overlay limits, so a tiny image on a large
    /// canvas stops at the largest scale. A missing image is an error.
    pub fn fit_to_canvas(&mut self, index: usize, canvas_w: u32, canvas_h: u32) -> Result<()> {
        crate::limits::check_canvas(canvas_w, canvas_h)?;
        let image = self
            .items
            .get(index)
            .ok_or(Error::Index { what: "reference" })?;
        if image.missing || image.width == 0 || image.height == 0 {
            return Err(Error::BadReference);
        }
        let (width, height) = (image.width as f32, image.height as f32);
        let (canvas_w, canvas_h) = (canvas_w as f32, canvas_h as f32);
        let scale = (canvas_w / width)
            .min(canvas_h / height)
            .clamp(MIN_REFERENCE_SCALE, MAX_REFERENCE_SCALE);
        let x = ((canvas_w - width * scale) / 2.0).clamp(-MAX_OFFSET, MAX_OFFSET);
        let y = ((canvas_h - height * scale) / 2.0).clamp(-MAX_OFFSET, MAX_OFFSET);
        self.edit(index, |image| {
            image.scale = scale;
            image.x = x;
            image.y = y;
        })
    }

    /// Move an unlocked canvas overlay by a document-pixel delta.
    pub fn nudge(&mut self, index: usize, dx: f32, dy: f32) -> bool {
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        let Some(image) = self.items.get_mut(index) else {
            return false;
        };
        if image.locked || image.panel || !image.visible || image.missing {
            return false;
        }
        image.x = (image.x + dx).clamp(-MAX_OFFSET, MAX_OFFSET);
        image.y = (image.y + dy).clamp(-MAX_OFFSET, MAX_OFFSET);
        self.touch();
        true
    }

    /// Topmost unlocked overlay under a document point, or none.
    pub fn hit_movable(&self, x: f32, y: f32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.items
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, image)| {
                if image.can_move() && image.contains(x, y) {
                    Some(index)
                } else {
                    None
                }
            })
    }

    /// Color the eyedropper should use.
    ///
    /// `document` is the composited pixel, or `None` outside the canvas.
    /// A reference contributes only when its Sample flag is on. An overlay
    /// above the art wins. An opaque document pixel wins over a reference
    /// that sits behind the art. Export never calls this.
    pub fn pick_color(&self, document: Option<Rgba>, x: i32, y: i32) -> Option<Rgba> {
        let dx = x as f32 + 0.5;
        let dy = y as f32 + 0.5;
        if let Some(color) = self.top_sample(dx, dy, false) {
            return Some(color);
        }
        if document.is_some_and(|color| color.a > 0) {
            return document;
        }
        if let Some(color) = self.top_sample(dx, dy, true) {
            return Some(color);
        }
        document
    }

    /// Nearest-neighbor copy of one reference onto a canvas-sized buffer.
    ///
    /// This is the explicit way those pixels enter a document. The reference
    /// itself stays out of [`crate::Document::composite`].
    pub fn bake(&self, index: usize, canvas_w: u32, canvas_h: u32) -> Result<Vec<Rgba>> {
        crate::limits::check_canvas(canvas_w, canvas_h)?;
        let image = self
            .items
            .get(index)
            .ok_or(Error::Index { what: "reference" })?;
        if image.missing {
            return Err(Error::Io("the reference file is missing".to_string()));
        }
        let mut out = vec![Rgba::CLEAR; (canvas_w as usize) * (canvas_h as usize)];
        if image.scale <= 0.0 || image.width == 0 || image.height == 0 {
            return Ok(out);
        }
        for y in 0..canvas_h {
            for x in 0..canvas_w {
                let Some(color) = image.sample_pixel(x as f32 + 0.5, y as f32 + 0.5) else {
                    continue;
                };
                out[(y * canvas_w + x) as usize] = color.fade(image.opacity);
            }
        }
        Ok(out)
    }

    pub fn overlays(&self) -> impl Iterator<Item = Overlay<'_>> {
        let active = self.active;
        self.items
            .iter()
            .enumerate()
            .filter_map(move |(index, image)| {
                if !image.shown_on_canvas() {
                    return None;
                }
                Some(Overlay {
                    id: image.id,
                    pixels: &image.pixels,
                    width: image.width,
                    height: image.height,
                    x: image.x,
                    y: image.y,
                    scale: image.scale,
                    opacity: image.opacity,
                    behind: image.behind,
                    active: Some(index) == active,
                })
            })
    }

    /// RGBA bytes for a texture. Missing and empty images return `None`.
    pub fn texture_rgba(&self, id: u64) -> Option<(u32, u32, Vec<u8>)> {
        let image = self.items.iter().find(|image| image.id == id)?;
        if image.missing || image.width == 0 || image.height == 0 {
            return None;
        }
        let count = (image.width as usize).checked_mul(image.height as usize)?;
        if image.pixels.len() != count {
            return None;
        }
        let mut bytes = Vec::with_capacity(count * 4);
        for pixel in &image.pixels {
            bytes.extend_from_slice(&pixel.to_bytes());
        }
        Some((image.width, image.height, bytes))
    }

    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.items.iter().map(|image| image.id)
    }

    /// Panel images that should float, with their id and opacity.
    pub fn panels(&self) -> impl Iterator<Item = (u64, u8, bool, u32, u32, String)> + '_ {
        self.items
            .iter()
            .filter(|image| image.panel && image.visible && !image.missing)
            .map(|image| {
                let name = image
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "reference".to_string());
                (
                    image.id,
                    image.opacity,
                    image.sample,
                    image.width,
                    image.height,
                    name,
                )
            })
    }

    /// Map a point inside a panel image to a source pixel. `u` and `v` are 0–1.
    pub fn panel_color(&self, id: u64, u: f32, v: f32) -> Option<Rgba> {
        if !u.is_finite() || !v.is_finite() {
            return None;
        }
        let image = self.items.iter().find(|image| image.id == id)?;
        if !image.sample || !image.visible || image.missing || !image.panel {
            return None;
        }
        if image.width == 0 || image.height == 0 {
            return None;
        }
        let px = (u.clamp(0.0, 0.999) * image.width as f32).floor() as u32;
        let py = (v.clamp(0.0, 0.999) * image.height as f32).floor() as u32;
        let index = (py as usize) * (image.width as usize) + (px as usize);
        let color = *image.pixels.get(index)?;
        if color.a == 0 {
            None
        } else {
            Some(color)
        }
    }

    /// Write `<document>.oxypix.refs`, or delete it when the list is empty.
    pub fn write_sidecar(&self, document: &Path) -> Result<()> {
        let path = sidecar_path(document);
        if self.items.is_empty() {
            return remove_if_present(&path);
        }
        let bytes = self.encode(document_dir(document))?;
        let mut tmp_name = path.as_os_str().to_os_string();
        tmp_name.push(".tmp");
        let tmp = PathBuf::from(tmp_name);
        std::fs::write(&tmp, &bytes).map_err(Error::io)?;
        if let Err(err) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(Error::io(err));
        }
        Ok(())
    }

    /// Load placement from the sidecar. A missing sidecar is an empty list.
    ///
    /// A missing image keeps its placement and is marked missing. A corrupt
    /// sidecar is an error and does not invent images.
    pub fn read_sidecar(document: &Path) -> Result<Self> {
        let path = sidecar_path(document);
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::new()),
            Err(err) => return Err(Error::io(err)),
        };
        if meta.len() > MAX_SIDECAR_BYTES {
            return Err(Error::Limit {
                what: "reference sidecar bytes",
                value: meta.len(),
                max: MAX_SIDECAR_BYTES,
            });
        }
        let bytes = std::fs::read(&path).map_err(Error::io)?;
        if bytes.len() as u64 > MAX_SIDECAR_BYTES {
            return Err(Error::Limit {
                what: "reference sidecar bytes",
                value: bytes.len() as u64,
                max: MAX_SIDECAR_BYTES,
            });
        }
        let records = parse_sidecar(&bytes)?;
        let dir = document_dir(document);
        let mut refs = Self::new();
        for mut record in records {
            if record.path.is_relative() {
                record.path = dir.join(&record.path);
            }
            let mut image = match refs.decode_file(&record.path) {
                Ok(image) => image,
                Err(_) => {
                    let image = Image {
                        id: refs.next_id,
                        path: record.path,
                        width: 0,
                        height: 0,
                        pixels: Vec::new(),
                        missing: true,
                        visible: true,
                        opacity: 255,
                        locked: false,
                        behind: false,
                        panel: false,
                        sample: false,
                        x: 0.0,
                        y: 0.0,
                        scale: 1.0,
                    };
                    refs.next_id = refs.next_id.saturating_add(1);
                    image
                }
            };
            image.visible = record.visible;
            image.opacity = record.opacity;
            image.locked = record.locked;
            image.behind = record.behind;
            image.panel = record.panel;
            image.sample = record.sample;
            image.x = record.x;
            image.y = record.y;
            image.scale = record.scale;
            refs.items.push(image);
        }
        if !refs.items.is_empty() {
            refs.active = Some(0);
            refs.revision = 1;
        }
        Ok(refs)
    }

    fn edit(&mut self, index: usize, change: impl FnOnce(&mut Image)) -> Result<()> {
        let image = self
            .items
            .get_mut(index)
            .ok_or(Error::Index { what: "reference" })?;
        change(image);
        self.touch();
        Ok(())
    }

    fn touch(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    fn decode_file(&mut self, path: &Path) -> Result<Image> {
        let bytes = read_capped(path)?;
        let (width, height, pixels) = decode_reference(&bytes)?;
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        Ok(Image {
            id,
            path: path.to_path_buf(),
            width,
            height,
            pixels,
            missing: false,
            visible: true,
            opacity: 255,
            locked: false,
            behind: true,
            panel: false,
            sample: false,
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        })
    }

    fn top_sample(&self, x: f32, y: f32, behind: bool) -> Option<Rgba> {
        self.items.iter().rev().find_map(|image| {
            if image.behind != behind || !image.sample || !image.shown_on_canvas() {
                return None;
            }
            image.sample_pixel(x, y)
        })
    }

    /// An image inside `dir` is written relative to it, so the folder can move.
    fn encode(&self, dir: &Path) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.extend(format!("{MAGIC}\n{}\n", self.items.len()).into_bytes());
        for image in &self.items {
            let stored = image.path.strip_prefix(dir).unwrap_or(&image.path);
            let path = stored.to_str().ok_or(Error::BadReference)?.as_bytes();
            if path.is_empty() || path.len() > MAX_PATH_BYTES || path.contains(&b'\n') {
                return Err(Error::BadReference);
            }
            out.extend(format!("{}\n", path.len()).into_bytes());
            out.extend_from_slice(path);
            out.push(b'\n');
            out.extend(
                format!(
                    "{} {} {} {} {} {}\n",
                    flag(image.visible),
                    image.opacity,
                    flag(image.locked),
                    flag(image.behind),
                    flag(image.panel),
                    flag(image.sample),
                )
                .into_bytes(),
            );
            out.extend(format!("{} {} {}\n", image.x, image.y, image.scale).into_bytes());
        }
        Ok(out)
    }
}

impl Image {
    fn shown_on_canvas(&self) -> bool {
        self.visible && !self.panel && !self.missing && self.opacity > 0 && self.scale > 0.0
    }

    fn can_move(&self) -> bool {
        self.shown_on_canvas() && !self.locked
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        if self.width == 0 || self.height == 0 || self.scale <= 0.0 {
            return false;
        }
        let u = (x - self.x) / self.scale;
        let v = (y - self.y) / self.scale;
        u >= 0.0 && v >= 0.0 && u < self.width as f32 && v < self.height as f32
    }

    fn sample_pixel(&self, x: f32, y: f32) -> Option<Rgba> {
        if !self.contains(x, y) {
            return None;
        }
        let px = ((x - self.x) / self.scale).floor() as usize;
        let py = ((y - self.y) / self.scale).floor() as usize;
        if px >= self.width as usize || py >= self.height as usize {
            return None;
        }
        let color = *self.pixels.get(py * self.width as usize + px)?;
        if color.a == 0 {
            None
        } else {
            Some(color)
        }
    }
}

/// `<file>.oxypix` becomes `<file>.oxypix.refs` in the same directory.
pub fn sidecar_path(document: &Path) -> PathBuf {
    let mut name = document
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".refs");
    match document.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// The folder a sidecar's relative paths start from.
fn document_dir(document: &Path) -> &Path {
    document.parent().unwrap_or(Path::new(""))
}

struct Record {
    path: PathBuf,
    visible: bool,
    opacity: u8,
    locked: bool,
    behind: bool,
    panel: bool,
    sample: bool,
    x: f32,
    y: f32,
    scale: f32,
}

fn parse_sidecar(bytes: &[u8]) -> Result<Vec<Record>> {
    let mut cursor = Cursor { bytes, pos: 0 };
    let magic = cursor.line()?;
    if magic != MAGIC.as_bytes() {
        return Err(Error::BadReference);
    }
    let count = parse_u32(cursor.line()?)?;
    if count as usize > MAX_REFERENCES {
        return Err(Error::Limit {
            what: "references",
            value: u64::from(count),
            max: MAX_REFERENCES as u64,
        });
    }
    let mut records = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let path_len = parse_u32(cursor.line()?)? as usize;
        if path_len == 0 || path_len > MAX_PATH_BYTES {
            return Err(Error::BadReference);
        }
        let path_bytes = cursor.take(path_len)?.to_vec();
        let newline = cursor.take(1)?;
        if newline != b"\n" {
            return Err(Error::BadReference);
        }
        let path = String::from_utf8(path_bytes).map_err(|_| Error::BadReference)?;
        if path.contains('\n') || path.contains('\0') {
            return Err(Error::BadReference);
        }
        let flags = cursor.line()?;
        let mut parts = flags.split(|byte| *byte == b' ');
        let visible = parse_flag(parts.next().ok_or(Error::BadReference)?)?;
        let opacity = parse_u8(parts.next().ok_or(Error::BadReference)?)?;
        let locked = parse_flag(parts.next().ok_or(Error::BadReference)?)?;
        let behind = parse_flag(parts.next().ok_or(Error::BadReference)?)?;
        let panel = parse_flag(parts.next().ok_or(Error::BadReference)?)?;
        let sample = parse_flag(parts.next().ok_or(Error::BadReference)?)?;
        if parts.next().is_some() {
            return Err(Error::BadReference);
        }
        let place = cursor.line()?;
        let mut place = place.split(|byte| *byte == b' ');
        let x = parse_finite(place.next().ok_or(Error::BadReference)?)?;
        let y = parse_finite(place.next().ok_or(Error::BadReference)?)?;
        let scale = parse_finite(place.next().ok_or(Error::BadReference)?)?;
        if place.next().is_some() {
            return Err(Error::BadReference);
        }
        if !(-MAX_OFFSET..=MAX_OFFSET).contains(&x) || !(-MAX_OFFSET..=MAX_OFFSET).contains(&y) {
            return Err(Error::BadReference);
        }
        if !(MIN_REFERENCE_SCALE..=MAX_REFERENCE_SCALE).contains(&scale) {
            return Err(Error::BadReference);
        }
        records.push(Record {
            path: PathBuf::from(path),
            visible,
            opacity,
            locked,
            behind,
            panel,
            sample,
            x,
            y,
            scale,
        });
    }
    if cursor.pos != cursor.bytes.len() {
        return Err(Error::BadReference);
    }
    Ok(records)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn line(&mut self) -> Result<&'a [u8]> {
        if self.pos >= self.bytes.len() {
            return Err(Error::BadReference);
        }
        let rest = &self.bytes[self.pos..];
        let end = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(Error::BadReference)?;
        let mut line = &rest[..end];
        self.pos += end + 1;
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        Ok(line)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).ok_or(Error::BadReference)?;
        if end > self.bytes.len() {
            return Err(Error::BadReference);
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }
}

fn decode_reference(bytes: &[u8]) -> Result<(u32, u32, Vec<Rgba>)> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Limit {
            what: "file bytes",
            value: bytes.len() as u64,
            max: MAX_FILE_BYTES,
        });
    }
    let format = image::guess_format(bytes).map_err(|_| Error::BadReferenceImage)?;
    let image = match format {
        ImageFormat::Png => decode_png(bytes)?,
        ImageFormat::Jpeg => decode_jpeg(bytes)?,
        _ => return Err(Error::BadReferenceImage),
    };
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 || width > MAX_REFERENCE_SIDE || height > MAX_REFERENCE_SIDE {
        return Err(Error::Limit {
            what: "reference side",
            value: u64::from(width.max(height)),
            max: u64::from(MAX_REFERENCE_SIDE),
        });
    }
    let pixels = rgba
        .pixels()
        .map(|pixel| Rgba::new(pixel[0], pixel[1], pixel[2], pixel[3]))
        .collect::<Vec<_>>();
    let expected = (width as usize).saturating_mul(height as usize);
    if pixels.len() != expected {
        return Err(Error::Truncated);
    }
    Ok((width, height, pixels))
}

fn decode_png(bytes: &[u8]) -> Result<DynamicImage> {
    let decoder = PngDecoder::new(std::io::Cursor::new(bytes)).map_err(Error::image)?;
    decode_limited(decoder)
}

fn decode_jpeg(bytes: &[u8]) -> Result<DynamicImage> {
    let decoder = JpegDecoder::new(std::io::Cursor::new(bytes)).map_err(Error::image)?;
    decode_limited(decoder)
}

fn decode_limited(mut decoder: impl ImageDecoder) -> Result<DynamicImage> {
    decoder
        .set_limits(reference_limits())
        .map_err(Error::image)?;
    DynamicImage::from_decoder(decoder).map_err(Error::image)
}

fn reference_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_REFERENCE_SIDE);
    limits.max_image_height = Some(MAX_REFERENCE_SIDE);
    limits.max_alloc = Some(32 * 1024 * 1024);
    limits
}

fn read_capped(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(Error::io)?;
    check_file_len(meta.len())?;
    let bytes = std::fs::read(path).map_err(Error::io)?;
    check_file_len(bytes.len() as u64)?;
    Ok(bytes)
}

fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(Error::io(err)),
    }
}

fn flag(value: bool) -> u8 {
    u8::from(value)
}

fn parse_u32(bytes: &[u8]) -> Result<u32> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::BadReference)?;
    text.parse::<u32>().map_err(|_| Error::BadReference)
}

fn parse_u8(bytes: &[u8]) -> Result<u8> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::BadReference)?;
    text.parse::<u8>().map_err(|_| Error::BadReference)
}

fn parse_flag(bytes: &[u8]) -> Result<bool> {
    match bytes {
        b"0" => Ok(false),
        b"1" => Ok(true),
        _ => Err(Error::BadReference),
    }
}

fn parse_finite(bytes: &[u8]) -> Result<f32> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::BadReference)?;
    let value = text.parse::<f32>().map_err(|_| Error::BadReference)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::BadReference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::{
        export_bmp_bytes, export_gif_bytes, export_png_bytes, export_spritesheet_bytes, Editor,
    };
    use image::codecs::jpeg::JpegEncoder;
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(name: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("oxypixel-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path, width: u32, height: u32, pixels: &[Rgba]) {
        let raw: Vec<u8> = pixels.iter().flat_map(|pixel| pixel.to_bytes()).collect();
        let file = std::fs::File::create(path).unwrap();
        PngEncoder::new(file)
            .write_image(&raw, width, height, ExtendedColorType::Rgba8)
            .unwrap();
    }

    fn red_png(dir: &Path) -> PathBuf {
        let path = dir.join("ref.png");
        write_png(&path, 2, 1, &[Rgba::new(255, 0, 0, 255), Rgba::CLEAR]);
        path
    }

    fn painted() -> Document {
        let mut doc = Document::new(2, 1).unwrap();
        doc.frames_mut()[0].cels[0][0] = Rgba::new(0, 0, 255, 255);
        doc
    }

    #[test]
    fn sidecar_sits_beside_the_document() {
        let path = sidecar_path(Path::new("/drawings/sprite.oxypix"));
        assert_eq!(path, PathBuf::from("/drawings/sprite.oxypix.refs"));
        assert_eq!(
            sidecar_path(Path::new("sprite.oxypix")),
            PathBuf::from("sprite.oxypix.refs")
        );
    }

    #[test]
    fn a_new_reference_sits_behind_the_art() {
        let dir = scratch("behind");
        let mut refs = References::new();
        refs.add_file(&red_png(&dir)).unwrap();
        assert!(refs.behind(0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fit_to_canvas_scales_down_and_centers() {
        let dir = scratch("fit");
        let wide = dir.join("wide.png");
        write_png(&wide, 64, 32, &vec![Rgba::new(255, 0, 0, 255); 64 * 32]);
        let mut refs = References::new();
        refs.add_file(&wide).unwrap();
        refs.fit_to_canvas(0, 32, 32).unwrap();
        assert_eq!(refs.scale(0), 0.5);
        assert_eq!(refs.placement(0), Some((0.0, 8.0)));

        // A large image on a small canvas goes below the old 0.25 floor.
        let big = dir.join("big.png");
        write_png(&big, 2048, 1, &vec![Rgba::new(0, 255, 0, 255); 2048]);
        refs.add_file(&big).unwrap();
        refs.fit_to_canvas(1, 32, 32).unwrap();
        assert_eq!(refs.scale(1), MIN_REFERENCE_SCALE);
        assert_eq!(refs.placement(1).unwrap().0, 0.0);

        // A tiny image stops at the largest scale and stays centered.
        refs.add_file(&red_png(&dir)).unwrap();
        refs.fit_to_canvas(2, 1024, 1024).unwrap();
        assert_eq!(refs.scale(2), MAX_REFERENCE_SCALE);
        assert_eq!(refs.placement(2), Some((504.0, 508.0)));

        assert!(refs.fit_to_canvas(9, 32, 32).is_err());
        assert!(refs.fit_to_canvas(0, 0, 32).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_typed_width_or_height_keeps_the_aspect_ratio() {
        let dir = scratch("size");
        let wide = dir.join("wide.png");
        write_png(&wide, 200, 100, &vec![Rgba::new(255, 0, 0, 255); 200 * 100]);
        let mut refs = References::new();
        refs.add_file(&wide).unwrap();
        assert_eq!(refs.display_size(0), Some((200.0, 100.0)));
        refs.set_display_width(0, 50.0).unwrap();
        assert_eq!(refs.scale(0), 0.25);
        assert_eq!(refs.display_size(0), Some((50.0, 25.0)));
        refs.set_display_height(0, 100.0).unwrap();
        assert_eq!(refs.display_size(0), Some((200.0, 100.0)));
        // Past the scale limits the size stops at the limit.
        refs.set_display_width(0, 1.0).unwrap();
        assert_eq!(refs.scale(0), MIN_REFERENCE_SCALE);
        assert!(refs.set_display_width(0, 0.0).is_err());
        assert!(refs.set_display_width(0, f32::NAN).is_err());
        assert!(refs.set_display_width(5, 10.0).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scale_reaches_one_sixty_fourth_and_round_trips() {
        let dir = scratch("small-scale");
        let doc_path = dir.join("sprite.oxypix");
        let mut refs = References::new();
        refs.add_file(&red_png(&dir)).unwrap();
        refs.set_scale(0, 0.001).unwrap();
        assert_eq!(refs.scale(0), MIN_REFERENCE_SCALE);
        refs.write_sidecar(&doc_path).unwrap();
        let loaded = References::read_sidecar(&doc_path).unwrap();
        assert_eq!(loaded.scale(0), MIN_REFERENCE_SCALE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reference_beside_the_document_follows_the_folder() {
        let root = scratch("relative");
        let before = root.join("before");
        std::fs::create_dir_all(before.join("art")).unwrap();
        let inside = before.join("art").join("ref.png");
        write_png(&inside, 1, 1, &[Rgba::new(255, 0, 0, 255)]);
        let outside = red_png(&root);
        let mut refs = References::new();
        refs.add_file(&inside).unwrap();
        refs.add_file(&outside).unwrap();
        refs.write_sidecar(&before.join("sprite.oxypix")).unwrap();

        let text = std::fs::read_to_string(before.join("sprite.oxypix.refs")).unwrap();
        let inside_name = Path::new("art").join("ref.png");
        assert!(text.contains(&format!("\n{}\n", inside_name.to_str().unwrap())));
        assert!(text.contains(outside.to_str().unwrap()));
        assert!(!text.contains(before.to_str().unwrap()));

        let after = root.join("after");
        std::fs::rename(&before, &after).unwrap();
        let loaded = References::read_sidecar(&after.join("sprite.oxypix")).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(!loaded.missing(0));
        assert!(!loaded.missing(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn png_and_placement_roundtrip_and_a_missing_image_is_kept() {
        let dir = scratch("roundtrip");
        let png = red_png(&dir);
        let doc_path = dir.join("sprite.oxypix");
        let mut refs = References::new();
        refs.add_file(&png).unwrap();
        refs.set_opacity(0, 128).unwrap();
        refs.set_behind(0, true).unwrap();
        refs.set_sample(0, true).unwrap();
        refs.set_scale(0, 2.0).unwrap();
        assert!(refs.nudge(0, 4.0, -2.0));
        refs.set_locked(0, true).unwrap();
        refs.set_panel(0, true).unwrap();
        refs.set_panel(0, false).unwrap();
        refs.set_visible(0, false).unwrap();
        refs.write_sidecar(&doc_path).unwrap();

        let mut loaded = References::read_sidecar(&doc_path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(!loaded.missing(0));
        assert_eq!(loaded.opacity(0), 128);
        assert!(loaded.locked(0));
        assert!(loaded.behind(0));
        assert!(!loaded.panel(0));
        assert!(loaded.samples(0));
        assert!(!loaded.visible(0));
        assert_eq!(loaded.scale(0), 2.0);
        assert_eq!(loaded.pick_color(None, 4, -2), None);
        loaded.set_visible(0, true).unwrap();
        let clear = Some(Rgba::CLEAR);
        let blue = Some(Rgba::new(0, 0, 255, 255));
        assert_eq!(loaded.pick_color(blue, 4, -2), blue);
        assert_eq!(
            loaded.pick_color(clear, 4, -2),
            Some(Rgba::new(255, 0, 0, 255))
        );

        std::fs::remove_file(&png).unwrap();
        let missing = References::read_sidecar(&doc_path).unwrap();
        assert!(missing.missing(0));
        assert_eq!(missing.opacity(0), 128);
        assert!(missing.bake(0, 2, 1).is_err());
    }

    #[test]
    fn jpeg_decodes_and_bmp_is_rejected() {
        let dir = scratch("jpeg");
        let jpeg = dir.join("ref.jpg");
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 90)
            .write_image(&[255, 0, 0], 1, 1, ExtendedColorType::Rgb8)
            .unwrap();
        std::fs::write(&jpeg, &bytes).unwrap();
        let mut refs = References::new();
        refs.add_file(&jpeg).unwrap();
        let picked = refs.pick_color(None, 0, 0);
        assert!(picked.is_none());
        refs.set_sample(0, true).unwrap();
        let picked = refs.pick_color(None, 0, 0).unwrap();
        assert!(picked.r > 200);
        assert!(picked.a == 255);

        let bmp = dir.join("ref.bmp");
        let mut bmp_file = std::fs::File::create(&bmp).unwrap();
        image::codecs::bmp::BmpEncoder::new(&mut bmp_file)
            .write_image(&[0, 255, 0, 255], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        let err = refs.add_file(&bmp).unwrap_err();
        assert!(matches!(err, Error::BadReferenceImage));
    }

    #[test]
    fn corrupt_sidecar_is_an_error_and_empty_list_deletes_it() {
        let dir = scratch("bad");
        let doc = dir.join("sprite.oxypix");
        let side = sidecar_path(&doc);
        std::fs::write(&side, b"OXYREFS1\n1\n").unwrap();
        assert!(matches!(
            References::read_sidecar(&doc).unwrap_err(),
            Error::BadReference
        ));
        std::fs::write(&side, b"OXYREFS1\n99\n").unwrap();
        assert!(matches!(
            References::read_sidecar(&doc).unwrap_err(),
            Error::Limit { .. }
        ));
        std::fs::write(&side, b"NOPE\n0\n").unwrap();
        assert!(References::read_sidecar(&doc).is_err());

        let refs = References::new();
        refs.write_sidecar(&doc).unwrap();
        assert!(!side.exists());
        assert!(References::read_sidecar(&doc).unwrap().is_empty());
    }

    #[test]
    fn sample_flag_gates_the_eyedropper_and_stacking_respects_the_art() {
        let dir = scratch("sample");
        let png = red_png(&dir);
        let mut refs = References::new();
        refs.add_file(&png).unwrap();
        refs.set_behind(0, false).unwrap();
        let document = Some(Rgba::new(0, 0, 255, 255));
        let clear = Some(Rgba::CLEAR);
        assert_eq!(refs.pick_color(document, 0, 0), document);
        refs.set_sample(0, true).unwrap();
        assert_eq!(
            refs.pick_color(document, 0, 0),
            Some(Rgba::new(255, 0, 0, 255))
        );
        refs.set_behind(0, true).unwrap();
        assert_eq!(refs.pick_color(document, 0, 0), document);
        assert_eq!(
            refs.pick_color(clear, 0, 0),
            Some(Rgba::new(255, 0, 0, 255))
        );
        assert_eq!(refs.pick_color(clear, 1, 0), clear);
        refs.set_panel(0, true).unwrap();
        assert_eq!(refs.pick_color(clear, 0, 0), clear);
    }

    #[test]
    fn locked_and_hidden_overlays_do_not_move() {
        let dir = scratch("move");
        let png = red_png(&dir);
        let mut refs = References::new();
        refs.add_file(&png).unwrap();
        assert_eq!(refs.hit_movable(0.5, 0.5), Some(0));
        assert!(refs.nudge(0, 3.0, 1.0));
        refs.set_locked(0, true).unwrap();
        assert_eq!(refs.hit_movable(3.5, 1.5), None);
        assert!(!refs.nudge(0, 5.0, 0.0));
        refs.set_locked(0, false).unwrap();
        refs.set_visible(0, false).unwrap();
        assert!(!refs.nudge(0, 1.0, 0.0));
        assert!(!refs.nudge(0, f32::NAN, 0.0));
    }

    #[test]
    fn export_and_merge_ignore_references_until_a_layer_is_copied() {
        let dir = scratch("export");
        let png = red_png(&dir);
        let doc = painted();
        let png_before = export_png_bytes(&doc, 0).unwrap();
        let bmp_before = export_bmp_bytes(&doc, 0).unwrap();
        let gif_before = export_gif_bytes(&doc).unwrap();
        let sheet_before = export_spritesheet_bytes(&doc).unwrap();
        let composite = doc.composite(0).unwrap();

        let mut refs = References::new();
        refs.add_file(&png).unwrap();
        refs.set_sample(0, true).unwrap();
        assert_eq!(export_png_bytes(&doc, 0).unwrap(), png_before);
        assert_eq!(export_bmp_bytes(&doc, 0).unwrap(), bmp_before);
        assert_eq!(export_gif_bytes(&doc).unwrap(), gif_before);
        assert_eq!(export_spritesheet_bytes(&doc).unwrap(), sheet_before);
        assert_eq!(doc.composite(0).unwrap(), composite);

        let mut editor = Editor::new(2, 1).unwrap();
        editor
            .insert_layer_pixels("Top", &[Rgba::new(0, 255, 0, 255), Rgba::CLEAR])
            .unwrap();
        editor.merge_down().unwrap();
        let merged = editor.document().composite(0).unwrap();
        assert_eq!(merged[0], Rgba::new(0, 255, 0, 255));
        assert_ne!(merged[0].r, 255);

        let baked = refs.bake(0, 2, 1).unwrap();
        assert_eq!(baked[0], Rgba::new(255, 0, 0, 255));
        assert_eq!(baked[1], Rgba::CLEAR);
        assert_ne!(baked, composite);
    }

    #[test]
    fn the_list_stops_at_eight() {
        let dir = scratch("cap");
        let png = red_png(&dir);
        let mut refs = References::new();
        for _ in 0..MAX_REFERENCES {
            refs.add_file(&png).unwrap();
        }
        let err = refs.add_file(&png).unwrap_err();
        assert!(matches!(err, Error::Limit { .. }));
    }
}
