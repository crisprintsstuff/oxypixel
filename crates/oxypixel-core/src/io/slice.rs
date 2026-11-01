//! Cut a spritesheet into frames or layers.
//!
//! The grid starts `offset` pixels in from the top-left corner and steps by
//! the cell size plus the gap. Cells are read row by row. A cell that runs
//! past the sheet's edge is not a cell, so a ragged right or bottom edge is
//! ignored rather than padded.

use std::io::Cursor;
use std::path::Path;

use image::{ImageFormat, ImageReader, Limits};

use crate::color::{unpack, Rgba};
use crate::document::{default_palette, Document, Frame, LayerInfo};
use crate::error::{Error, Result};
use crate::limits::{
    check_canvas, clamp_duration, MAX_FRAMES, MAX_LAYERS, MAX_SHEET_PADDING, MAX_SHEET_WIDTH,
};

use super::oxypix::read_limited;

/// Most pixels in a sheet read for slicing, 64 MiB as RGBA.
pub const MAX_SLICE_SHEET_PIXELS: u64 = 16 * 1024 * 1024;

/// A whole image to cut up, as RGBA, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgba>,
}

/// Where the cells of a spritesheet are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceGrid {
    pub cell_width: u32,
    pub cell_height: u32,
    /// Pixels before the first column and above the first row.
    pub offset_x: u32,
    pub offset_y: u32,
    /// Pixels between columns and between rows.
    pub gap_x: u32,
    pub gap_y: u32,
    /// Keep cells with no visible pixel. Off drops them, wherever they are.
    pub keep_empty: bool,
}

/// What each cell becomes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliceInto {
    /// One frame per cell, on one layer.
    #[default]
    Frames,
    /// One layer per cell, in one frame. The first cell is the bottom layer.
    Layers,
}

impl SliceGrid {
    /// Cells of one size with no offset or gap. Empty cells are dropped.
    pub fn cells_of(cell_width: u32, cell_height: u32) -> Self {
        Self {
            cell_width,
            cell_height,
            offset_x: 0,
            offset_y: 0,
            gap_x: 0,
            gap_y: 0,
            keep_empty: false,
        }
    }

    /// A first guess for a sheet: square cells as tall as a one-row strip or
    /// as wide as a one-column strip, or else the whole image as one cell.
    pub fn guess(width: u32, height: u32) -> Self {
        let side = width.min(height);
        if side > 0 && width % side == 0 && height % side == 0 {
            Self::cells_of(side, side)
        } else {
            Self::cells_of(width, height)
        }
    }

    /// Columns and rows of whole cells that fit on a sheet.
    pub fn columns_rows(&self, width: u32, height: u32) -> (u32, u32) {
        let fit = |size: u32, offset: u32, cell: u32, gap: u32| {
            let room = u64::from(size).saturating_sub(u64::from(offset));
            let cell = u64::from(cell);
            if cell == 0 || room < cell {
                0
            } else {
                ((room - cell) / (cell + u64::from(gap)) + 1) as u32
            }
        };
        (
            fit(width, self.offset_x, self.cell_width, self.gap_x),
            fit(height, self.offset_y, self.cell_height, self.gap_y),
        )
    }

    fn check(&self) -> Result<()> {
        check_canvas(self.cell_width, self.cell_height)?;
        for (what, value) in [
            ("spritesheet gap", self.gap_x),
            ("spritesheet gap", self.gap_y),
        ] {
            if value > MAX_SHEET_PADDING {
                return Err(Error::Limit {
                    what,
                    value: u64::from(value),
                    max: u64::from(MAX_SHEET_PADDING),
                });
            }
        }
        Ok(())
    }
}

pub fn read_sheet(path: &Path) -> Result<Sheet> {
    decode_sheet(&read_limited(path)?)
}

/// A PNG, BMP, or GIF (its first frame) at up to `MAX_SHEET_WIDTH` on a side
/// and `MAX_SLICE_SHEET_PIXELS` in all, larger than a canvas may be.
pub fn decode_sheet(bytes: &[u8]) -> Result<Sheet> {
    let format = image::guess_format(bytes).map_err(|_| Error::UnsupportedFormat)?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Bmp | ImageFormat::Gif
    ) {
        return Err(Error::UnsupportedFormat);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SHEET_WIDTH);
    limits.max_image_height = Some(MAX_SHEET_WIDTH);
    limits.max_alloc = Some(MAX_SLICE_SHEET_PIXELS * 4 + 16 * 1024 * 1024);
    reader.limits(limits.clone());
    let (width, height) = reader.into_dimensions().map_err(Error::image)?;
    let area = u64::from(width) * u64::from(height);
    if area > MAX_SLICE_SHEET_PIXELS {
        return Err(Error::Limit {
            what: "pixels in a spritesheet",
            value: area,
            max: MAX_SLICE_SHEET_PIXELS,
        });
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let image = reader.decode().map_err(Error::image)?.to_rgba8();
    let (width, height) = image.dimensions();
    let pixels = unpack(image.as_raw()).ok_or(Error::Truncated)?;
    Ok(Sheet {
        width,
        height,
        pixels,
    })
}

/// The cells of a sheet, row by row, each `cell_width * cell_height` pixels.
pub fn sheet_cells(sheet: &Sheet, grid: &SliceGrid) -> Result<Vec<Vec<Rgba>>> {
    let (cw, ch) = (grid.cell_width as usize, grid.cell_height as usize);
    Ok(cell_corners(sheet, grid)?
        .into_iter()
        .map(|(x0, y0)| {
            let (x0, y0) = (x0 as usize, y0 as usize);
            let mut cell = Vec::with_capacity(cw * ch);
            for y in y0..y0 + ch {
                let start = y * sheet.width as usize + x0;
                cell.extend_from_slice(&sheet.pixels[start..start + cw]);
            }
            cell
        })
        .collect())
}

/// The top-left corner of each cell [`sheet_cells`] keeps, in the same order.
pub fn cell_corners(sheet: &Sheet, grid: &SliceGrid) -> Result<Vec<(u32, u32)>> {
    grid.check()?;
    if sheet.pixels.len() != sheet.width as usize * sheet.height as usize {
        return Err(Error::Truncated);
    }
    let (cols, rows) = grid.columns_rows(sheet.width, sheet.height);
    let (cw, ch) = (grid.cell_width as usize, grid.cell_height as usize);
    let mut corners = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let x0 = grid.offset_x + col * (grid.cell_width + grid.gap_x);
            let y0 = grid.offset_y + row * (grid.cell_height + grid.gap_y);
            let visible = || {
                (y0 as usize..y0 as usize + ch).any(|y| {
                    let start = y * sheet.width as usize + x0 as usize;
                    sheet.pixels[start..start + cw].iter().any(|p| p.a != 0)
                })
            };
            if grid.keep_empty || visible() {
                corners.push((x0, y0));
            }
        }
    }
    Ok(corners)
}

/// A new document of the sheet's cells, one per frame or one per layer.
/// Frames last `duration_ms` each.
pub fn slice_sheet(
    sheet: &Sheet,
    grid: &SliceGrid,
    into: SliceInto,
    duration_ms: u32,
) -> Result<Document> {
    let cells = sheet_cells(sheet, grid)?;
    if cells.is_empty() {
        return Err(Error::BadSheet(
            "no cell of that size fits, or every cell is empty".to_string(),
        ));
    }
    let max = match into {
        SliceInto::Frames => MAX_FRAMES,
        SliceInto::Layers => MAX_LAYERS,
    };
    if cells.len() > max {
        return Err(Error::Limit {
            what: match into {
                SliceInto::Frames => "frames",
                SliceInto::Layers => "layers",
            },
            value: cells.len() as u64,
            max: max as u64,
        });
    }
    let duration_ms = clamp_duration(duration_ms);
    let (layers, frames) = match into {
        SliceInto::Frames => (
            vec![LayerInfo::new("Sheet".to_string())],
            cells
                .into_iter()
                .map(|cell| Frame {
                    duration_ms,
                    name: String::new(),
                    cels: vec![cell.into()],
                })
                .collect(),
        ),
        SliceInto::Layers => (
            (1..=cells.len())
                .map(|n| LayerInfo::new(format!("Cell {n}")))
                .collect(),
            vec![Frame {
                duration_ms,
                name: String::new(),
                cels: cells.into_iter().map(Into::into).collect(),
            }],
        ),
    };
    Document::from_parts(
        grid.cell_width,
        grid.cell_height,
        layers,
        frames,
        0,
        0,
        default_palette(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet whose pixel at (x, y) has red x and green y.
    fn sheet(width: u32, height: u32) -> Sheet {
        let pixels = (0..height)
            .flat_map(|y| (0..width).map(move |x| Rgba::new(x as u8, y as u8, 0, 255)))
            .collect();
        Sheet {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn cells_step_by_size_plus_gap_from_the_offset() {
        let grid = SliceGrid {
            offset_x: 1,
            offset_y: 2,
            gap_x: 1,
            gap_y: 3,
            ..SliceGrid::cells_of(4, 2)
        };
        // 1 + 4 + 1 + 4 = 10 wide fits two columns; 14 still two, 15 three.
        assert_eq!(grid.columns_rows(10, 9), (2, 2));
        assert_eq!(grid.columns_rows(10, 8), (2, 1));
        assert_eq!(grid.columns_rows(14, 4), (2, 1));
        assert_eq!(grid.columns_rows(15, 4), (3, 1));
        assert_eq!(grid.columns_rows(4, 4), (0, 1), "the offset leaves no room");
        let cells = sheet_cells(&sheet(10, 9), &grid).unwrap();
        assert_eq!(cells.len(), 4);
        let corner = |cell: &Vec<Rgba>| (cell[0].r, cell[0].g);
        assert_eq!(
            cells.iter().map(corner).collect::<Vec<_>>(),
            [(1, 2), (6, 2), (1, 7), (6, 7)],
            "row by row"
        );
        assert_eq!((cells[3][7].r, cells[3][7].g), (9, 8), "the last pixel");
    }

    #[test]
    fn empty_cells_are_dropped_unless_kept() {
        let mut s = sheet(4, 2);
        for p in s.pixels.iter_mut().filter(|p| p.r >= 2) {
            *p = Rgba::CLEAR;
        }
        let grid = SliceGrid::cells_of(2, 2);
        assert_eq!(sheet_cells(&s, &grid).unwrap().len(), 1);
        assert_eq!(cell_corners(&s, &grid).unwrap(), [(0, 0)]);
        let keep = SliceGrid {
            keep_empty: true,
            ..grid
        };
        assert_eq!(sheet_cells(&s, &keep).unwrap().len(), 2);
    }

    #[test]
    fn a_sheet_becomes_frames_or_layers() {
        let s = sheet(6, 2);
        let grid = SliceGrid::cells_of(2, 2);
        let doc = slice_sheet(&s, &grid, SliceInto::Frames, 80).unwrap();
        assert_eq!((doc.width(), doc.height()), (2, 2));
        assert_eq!((doc.frame_count(), doc.layer_count()), (3, 1));
        assert_eq!(doc.frame_duration(2), Some(80));
        assert_eq!(doc.cel(2, 0).unwrap()[0].r, 4);
        let doc = slice_sheet(&s, &grid, SliceInto::Layers, 80).unwrap();
        assert_eq!((doc.frame_count(), doc.layer_count()), (1, 3));
        assert_eq!(doc.cel(0, 1).unwrap()[1].r, 3);
        assert_eq!(doc.layer(2).unwrap().name, "Cell 3");
    }

    #[test]
    fn bad_grids_are_errors_not_panics() {
        let s = sheet(8, 8);
        assert!(slice_sheet(&s, &SliceGrid::cells_of(0, 4), SliceInto::Frames, 100).is_err());
        assert!(slice_sheet(&s, &SliceGrid::cells_of(9, 4), SliceInto::Frames, 100).is_err());
        let gap = SliceGrid {
            gap_x: MAX_SHEET_PADDING + 1,
            ..SliceGrid::cells_of(2, 2)
        };
        assert!(slice_sheet(&s, &gap, SliceInto::Frames, 100).is_err());
        let wide = sheet(66, 1);
        let one = SliceGrid::cells_of(1, 1);
        assert!(
            slice_sheet(&wide, &one, SliceInto::Layers, 100).is_err(),
            "more cells than layers"
        );
        let short = Sheet {
            pixels: vec![Rgba::CLEAR; 3],
            ..sheet(2, 2)
        };
        assert!(sheet_cells(&short, &one).is_err());
        assert!(decode_sheet(b"not an image").is_err());
    }

    #[test]
    fn a_strip_guesses_square_cells() {
        assert_eq!(SliceGrid::guess(64, 16).cell_width, 16);
        assert_eq!(SliceGrid::guess(16, 48).cell_height, 16);
        assert_eq!(
            (
                SliceGrid::guess(30, 20).cell_width,
                SliceGrid::guess(30, 20).cell_height
            ),
            (30, 20)
        );
    }

    #[test]
    fn a_sheet_wider_than_a_canvas_decodes() {
        let s = sheet(2048, 2);
        let mut png = Vec::new();
        image::RgbaImage::from_raw(s.width, s.height, crate::color::pack(&s.pixels))
            .unwrap()
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let back = decode_sheet(&png).unwrap();
        assert_eq!((back.width, back.height), (2048, 2));
        let doc = slice_sheet(&back, &SliceGrid::cells_of(16, 2), SliceInto::Frames, 100).unwrap();
        assert_eq!(doc.frame_count(), 128);
    }
}
