//! OxyPack: pack sprites from many files into one atlas PNG and a JSON hash.
//!
//! The `oxypack` command and the OxyPixel editor's Pack sprites window both
//! use this library. It reads `.png`, `.bmp`, `.gif`, `.oxypix`, and
//! `.aseprite` files, packs their frames, and writes the atlas. It never opens
//! a window or a dialog.

pub mod aseprite;
mod files;
pub mod pack;

pub use files::{collect, load, pack_files, packable, write_atlas, Loaded, Packed};
pub use pack::{Atlas, FrameTag, PackOptions, Packer, Placement, Sprite};
