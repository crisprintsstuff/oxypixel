//! Document model, drawing tools, history, and file IO for OxyPixel.
//!
//! The GUI lives in the `oxypixel` binary. This crate has no windowing
//! dependency, so the editing rules can be tested on their own.

#![forbid(unsafe_code)]

mod animation;
mod clipboard;
mod color;
mod document;
mod draw;
mod editor;
mod error;
mod history;
mod io;
mod limits;

pub use animation::advance;
pub use color::{source_over, Rgba};
pub use document::{Document, LayerInfo};
pub use draw::{PixelRect, Transform};
pub use editor::{Editor, MouseButton, Tool};
pub use error::Error;
pub use io::Fit;
pub use io::{
    decode_first_rgba, export_bmp, export_bmp_bytes, export_gif, export_gif_bytes, export_palette,
    export_palette_string, export_png, export_png_bytes, export_png_bytes_scaled,
    export_png_scaled, export_spritesheet, export_spritesheet_bytes, export_spritesheet_scaled,
    fit_into, import_image, import_image_bytes, import_palette, import_palette_bytes, load_oxypix,
    load_oxypix_bytes, save_oxypix, save_oxypix_bytes, PaletteFormat,
};
pub use limits::{
    DEFAULT_FRAME_MS, FORMAT_VERSION, MAX_BRUSH, MAX_CEL_PIXELS, MAX_DIMENSION, MAX_FILE_BYTES,
    MAX_FRAMES, MAX_FRAME_MS, MAX_LAYERS, MAX_PALETTE, MAX_TOTAL_PIXELS, MIN_FRAME_MS,
};
