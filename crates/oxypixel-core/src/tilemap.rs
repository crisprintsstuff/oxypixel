//! Tilesets and the cels of tilemap layers.
//!
//! A tilemap layer names a tileset and keeps, in each frame, a grid of tile
//! references that covers the canvas from its top-left corner. Its pixels
//! are drawn from that grid, so compositing and export treat it like any
//! layer. Drawing pixels on it changes the tiles under them (every copy of a
//! tile changes with it), and drawing on an empty cell makes a new tile.

use std::ops::{Deref, DerefMut};

use crate::color::Rgba;
use crate::error::{Error, Result};

/// The tile is mirrored left to right.
pub const TILE_FLIP_X: u32 = 1 << 31;
/// The tile is mirrored top to bottom.
pub const TILE_FLIP_Y: u32 = 1 << 30;
/// The tile is mirrored across its top-left to bottom-right diagonal
/// (square tiles only), before the other flips.
pub const TILE_FLIP_D: u32 = 1 << 29;
/// The tile number in a grid entry. 0 is an empty cell; tile `n` is the
/// tileset's `n`th tile, counting from 1.
pub const TILE_ID_MASK: u32 = TILE_FLIP_D - 1;
/// The bits of a grid entry that are flips.
pub const TILE_FLIPS: u32 = TILE_FLIP_X | TILE_FLIP_Y | TILE_FLIP_D;

/// Most tilesets in a document.
pub const MAX_TILESETS: usize = 32;
/// Most tiles in one tileset.
pub const MAX_TILES: usize = 4096;
/// Widest, and tallest, tile.
pub const MAX_TILE_SIDE: u32 = 256;
/// Most pixels in all of one tileset's tiles together.
pub const MAX_TILESET_PIXELS: u64 = 4 * 1024 * 1024;

/// Square tiles of one size, numbered from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tileset {
    pub name: String,
    tile_width: u32,
    tile_height: u32,
    tiles: Vec<Vec<Rgba>>,
}

impl Tileset {
    pub fn new(name: String, tile_width: u32, tile_height: u32) -> Result<Self> {
        for side in [tile_width, tile_height] {
            if side == 0 || side > MAX_TILE_SIDE {
                return Err(Error::Limit {
                    what: "tile size",
                    value: u64::from(side),
                    max: u64::from(MAX_TILE_SIDE),
                });
            }
        }
        Ok(Self {
            name,
            tile_width,
            tile_height,
            tiles: Vec::new(),
        })
    }

    pub fn tile_width(&self) -> u32 {
        self.tile_width
    }

    pub fn tile_height(&self) -> u32 {
        self.tile_height
    }

    /// How many tiles there are. Tile numbers run from 1 to this.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Tile `id`'s pixels, row by row. `None` for 0 and numbers past the end.
    pub fn tile(&self, id: u32) -> Option<&[Rgba]> {
        let index = (id as usize).checked_sub(1)?;
        self.tiles.get(index).map(Vec::as_slice)
    }

    pub(crate) fn tile_mut(&mut self, id: u32) -> Option<&mut Vec<Rgba>> {
        let index = (id as usize).checked_sub(1)?;
        self.tiles.get_mut(index)
    }

    /// Add a tile and return its number.
    pub(crate) fn push(&mut self, pixels: Vec<Rgba>) -> Result<u32> {
        if self.tiles.len() >= MAX_TILES {
            return Err(Error::Limit {
                what: "tiles in a tileset",
                value: self.tiles.len() as u64 + 1,
                max: MAX_TILES as u64,
            });
        }
        let total = (self.tiles.len() as u64 + 1) * self.tile_pixels() as u64;
        if total > MAX_TILESET_PIXELS {
            return Err(Error::Limit {
                what: "pixels in a tileset",
                value: total,
                max: MAX_TILESET_PIXELS,
            });
        }
        if pixels.len() != self.tile_pixels() {
            return Err(Error::Truncated);
        }
        self.tiles.push(pixels);
        Ok(self.tiles.len() as u32)
    }

    pub(crate) fn tile_pixels(&self) -> usize {
        self.tile_width as usize * self.tile_height as usize
    }

    pub(crate) fn tiles(&self) -> &[Vec<Rgba>] {
        &self.tiles
    }

    /// Columns and rows of the grid that covers a canvas.
    pub fn grid_size(&self, width: u32, height: u32) -> (u32, u32) {
        (
            width.div_ceil(self.tile_width),
            height.div_ceil(self.tile_height),
        )
    }

    /// Whether `value` can be in a grid of this tileset.
    pub(crate) fn valid_entry(&self, value: u32) -> bool {
        let id = value & TILE_ID_MASK;
        let square_or_unflipped = value & TILE_FLIP_D == 0 || self.tile_width == self.tile_height;
        id as usize <= self.tiles.len() && (id != 0 || value == 0) && square_or_unflipped
    }

    /// Where in the tile the pixel `(dx, dy)` of a cell holding `value` comes from.
    pub(crate) fn source(&self, value: u32, dx: u32, dy: u32) -> (u32, u32) {
        let (tw, th) = (self.tile_width, self.tile_height);
        // Undo the flips in the reverse order they were made.
        let sy = if value & TILE_FLIP_Y != 0 {
            th - 1 - dy
        } else {
            dy
        };
        let sx = if value & TILE_FLIP_X != 0 {
            tw - 1 - dx
        } else {
            dx
        };
        if value & TILE_FLIP_D != 0 && tw == th {
            (sy, sx)
        } else {
            (sx, sy)
        }
    }

    /// Draw a grid onto a canvas-sized cel.
    pub(crate) fn render(&self, grid: &[u32], width: u32, height: u32, out: &mut [Rgba]) {
        let (cols, _) = self.grid_size(width, height);
        for y in 0..height {
            for x in 0..width {
                let (cx, cy) = (x / self.tile_width, y / self.tile_height);
                let value = grid
                    .get((cy * cols + cx) as usize)
                    .copied()
                    .unwrap_or_default();
                out[(y * width + x) as usize] = self.pixel(value, x, y);
            }
        }
    }

    /// The color a cell holding `value` gives the canvas pixel `(x, y)`.
    pub(crate) fn pixel(&self, value: u32, x: u32, y: u32) -> Rgba {
        let Some(tile) = self.tile(value & TILE_ID_MASK) else {
            return Rgba::CLEAR;
        };
        let (sx, sy) = self.source(value, x % self.tile_width, y % self.tile_height);
        tile[(sy * self.tile_width + sx) as usize]
    }
}

/// One layer's pixels in one frame, and on a tilemap layer the grid they
/// are drawn from. It reads as its pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cel {
    pub pixels: Vec<Rgba>,
    /// Row by row, `cols * rows` entries. `None` on other layers.
    pub tiles: Option<Vec<u32>>,
}

impl From<Vec<Rgba>> for Cel {
    fn from(pixels: Vec<Rgba>) -> Self {
        Self {
            pixels,
            tiles: None,
        }
    }
}

impl Deref for Cel {
    type Target = Vec<Rgba>;

    fn deref(&self) -> &Vec<Rgba> {
        &self.pixels
    }
}

impl DerefMut for Cel {
    fn deref_mut(&mut self) -> &mut Vec<Rgba> {
        &mut self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Tileset {
        let mut set = Tileset::new("t".into(), 2, 2).unwrap();
        let (a, b, c, d) = (
            Rgba::new(1, 0, 0, 255),
            Rgba::new(2, 0, 0, 255),
            Rgba::new(3, 0, 0, 255),
            Rgba::new(4, 0, 0, 255),
        );
        set.push(vec![a, b, c, d]).unwrap();
        set
    }

    #[test]
    fn flips_mirror_the_tile() {
        let set = set();
        let at = |value, x, y| set.pixel(value, x, y).r;
        assert_eq!(
            [at(1, 0, 0), at(1, 1, 0), at(1, 0, 1), at(1, 1, 1)],
            [1, 2, 3, 4]
        );
        let fx = 1 | TILE_FLIP_X;
        assert_eq!([at(fx, 0, 0), at(fx, 1, 0)], [2, 1]);
        let fy = 1 | TILE_FLIP_Y;
        assert_eq!([at(fy, 0, 0), at(fy, 0, 1)], [3, 1]);
        let fd = 1 | TILE_FLIP_D;
        assert_eq!(
            [at(fd, 1, 0), at(fd, 0, 1)],
            [3, 2],
            "the diagonal swaps x and y"
        );
        assert_eq!(at(0, 0, 0), 0);
        assert_eq!(at(7, 0, 0), 0, "a missing tile draws clear");
    }

    #[test]
    fn a_grid_covers_the_canvas_from_the_top_left() {
        let set = set();
        assert_eq!(set.grid_size(5, 4), (3, 2));
        let mut out = vec![Rgba::CLEAR; 3 * 3];
        set.render(&[0, 1 | TILE_FLIP_X, 1, 0], 3, 3, &mut out);
        let reds: Vec<u8> = out.iter().map(|p| p.r).collect();
        assert_eq!(reds, [0, 0, 2, 0, 0, 4, 1, 2, 0]);
        assert!(set.valid_entry(1 | TILE_FLIP_D));
        assert!(!set.valid_entry(2));
        assert!(!set.valid_entry(TILE_FLIP_X), "an empty cell has no flips");
        assert!(Tileset::new("x".into(), 0, 4).is_err());
    }
}
