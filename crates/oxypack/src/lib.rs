//! OxyPack: pack sprites from many files into one atlas PNG and a JSON hash.
//!
//! The `oxypack` command and the OxyPixel editor's Pack sprites window both
//! use this library. It reads `.png`, `.bmp`, `.gif`, `.oxypix`, and
//! `.aseprite` files, packs their frames, and writes the atlas. It never opens
//! a window or a dialog.

pub mod aseprite;
pub mod blend;
mod files;
pub mod pack;

pub use files::{collect, load, pack_files, packable, write_atlas, Loaded, Packed};
pub use pack::{Atlas, FrameTag, PackOptions, Packer, Placement, Sprite};

/// `n` and the noun, singular for exactly one: "1 file", "2 files".
pub fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::counted;

    #[test]
    fn counts_use_the_singular_only_for_one() {
        assert_eq!(counted(0, "file", "files"), "0 files");
        assert_eq!(counted(1, "file", "files"), "1 file");
        assert_eq!(counted(4, "sprite", "sprites"), "4 sprites");
    }
}
