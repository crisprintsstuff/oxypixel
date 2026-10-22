//! `.oxypix` documents, palette files, and PNG, BMP, and GIF import/export.

mod oxypix;
mod palette;
mod raster;

pub use oxypix::{load_oxypix, load_oxypix_bytes, save_oxypix, save_oxypix_bytes};
pub use palette::{
    export_palette, export_palette_string, import_palette, import_palette_bytes, PaletteFormat,
};
pub use raster::{
    decode_first_rgba, export_bmp, export_bmp_bytes, export_gif, export_gif_bytes, export_png,
    export_png_bytes, export_png_bytes_scaled, export_png_scaled, export_spritesheet,
    export_spritesheet_bytes, export_spritesheet_scaled, fit_into, import_image,
    import_image_bytes, Fit,
};
pub(crate) use raster::{decode_png_rgba, encode_rgba_png};
