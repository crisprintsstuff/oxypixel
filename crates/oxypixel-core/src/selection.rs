//! Selections that are not rectangles.
//!
//! A selection is its bounding [`PixelRect`] plus an optional mask, one `bool`
//! per pixel of that rectangle, row by row. `None` means every pixel of the
//! rectangle is selected, which is how a marquee and Select All are stored.

use crate::color::Rgba;
use crate::draw::{transform_pixels, PixelRect, Transform};

/// How a new marquee or wand click combines with the current selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectMode {
    /// The new area replaces the selection.
    #[default]
    Replace,
    /// The new area joins the selection (Shift).
    Add,
    /// The new area is taken out of the selection (Alt).
    Subtract,
}

/// One unit or merged edge of a selection outline, in document pixels.
/// Either `y0 == y1` (horizontal) or `x0 == x1` (vertical).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineEdge {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

/// Whether `(x, y)` is selected. Outside the rectangle is never selected.
pub(crate) fn mask_contains(rect: PixelRect, mask: Option<&[bool]>, x: i32, y: i32) -> bool {
    if !rect.contains(x, y) {
        return false;
    }
    match mask {
        None => true,
        Some(mask) => {
            let i = (y - rect.y) as usize * rect.width as usize + (x - rect.x) as usize;
            mask.get(i).copied().unwrap_or(false)
        }
    }
}

/// Trim the mask to the pixels it selects. `None` when it selects nothing.
/// A mask that fills its rectangle is dropped, so it is stored as a plain marquee.
pub(crate) fn normalize(
    rect: PixelRect,
    mask: Option<Vec<bool>>,
) -> Option<(PixelRect, Option<Vec<bool>>)> {
    let Some(mask) = mask else {
        return Some((rect, None));
    };
    let width = rect.width as usize;
    if mask.len() != width * rect.height as usize {
        return None;
    }
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for (i, _) in mask.iter().enumerate().filter(|(_, on)| **on) {
        let (x, y) = (i % width, i / width);
        left = left.min(x);
        right = right.max(x);
        top = top.min(y);
        bottom = bottom.max(y);
    }
    if left == usize::MAX {
        return None;
    }
    let trimmed = PixelRect {
        x: rect.x + left as i32,
        y: rect.y + top as i32,
        width: (right - left + 1) as u32,
        height: (bottom - top + 1) as u32,
    };
    let mut cropped = Vec::with_capacity(trimmed.width as usize * trimmed.height as usize);
    for y in top..=bottom {
        cropped.extend_from_slice(&mask[y * width + left..=y * width + right]);
    }
    if cropped.iter().all(|on| *on) {
        Some((trimmed, None))
    } else {
        Some((trimmed, Some(cropped)))
    }
}

/// The selection as one `bool` per canvas pixel. Parts off the canvas are dropped.
pub(crate) fn to_canvas(
    rect: PixelRect,
    mask: Option<&[bool]>,
    canvas_w: u32,
    canvas_h: u32,
) -> Vec<bool> {
    let mut grid = vec![false; canvas_w as usize * canvas_h as usize];
    for y in 0..canvas_h as i32 {
        for x in 0..canvas_w as i32 {
            if mask_contains(rect, mask, x, y) {
                grid[y as usize * canvas_w as usize + x as usize] = true;
            }
        }
    }
    grid
}

/// Combine a canvas-sized `area` with the current selection.
pub(crate) fn combine(
    current: Option<(PixelRect, Option<&[bool]>)>,
    area: Vec<bool>,
    mode: SelectMode,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<(PixelRect, Option<Vec<bool>>)> {
    let canvas = PixelRect {
        x: 0,
        y: 0,
        width: canvas_w,
        height: canvas_h,
    };
    let grid = match (mode, current) {
        (SelectMode::Subtract, None) => return None,
        (SelectMode::Replace, _) | (SelectMode::Add, None) => area,
        (_, Some((rect, mask))) => {
            let mut grid = to_canvas(rect, mask, canvas_w, canvas_h);
            for (cell, on) in grid.iter_mut().zip(area) {
                match mode {
                    SelectMode::Add => *cell |= on,
                    SelectMode::Subtract => *cell &= !on,
                    SelectMode::Replace => *cell = on,
                }
            }
            grid
        }
    };
    normalize(canvas, Some(grid))
}

/// The border between selected and unselected pixels, as horizontal and
/// vertical runs. A plain rectangle is its four sides.
pub fn outline(rect: PixelRect, mask: Option<&[bool]>) -> Vec<OutlineEdge> {
    let (w, h) = (rect.width as i32, rect.height as i32);
    if mask.is_none() {
        let (x1, y1) = (rect.x + w, rect.y + h);
        let side = |x0, y0, x1, y1| OutlineEdge { x0, y0, x1, y1 };
        return vec![
            side(rect.x, rect.y, x1, rect.y),
            side(rect.x, y1, x1, y1),
            side(rect.x, rect.y, rect.x, y1),
            side(x1, rect.y, x1, y1),
        ];
    }
    let at = |x: i32, y: i32| -> bool {
        x >= 0 && y >= 0 && x < w && y < h && mask_contains(rect, mask, rect.x + x, rect.y + y)
    };
    let mut edges = Vec::new();
    // Horizontal edges sit on row lines 0..=h, between the pixel above and below.
    for y in 0..=h {
        let mut run: Option<i32> = None;
        for x in 0..=w {
            let edge = x < w && at(x, y - 1) != at(x, y);
            match (edge, run) {
                (true, None) => run = Some(x),
                (false, Some(start)) => {
                    edges.push(OutlineEdge {
                        x0: rect.x + start,
                        y0: rect.y + y,
                        x1: rect.x + x,
                        y1: rect.y + y,
                    });
                    run = None;
                }
                _ => {}
            }
        }
    }
    // Vertical edges sit on column lines 0..=w.
    for x in 0..=w {
        let mut run: Option<i32> = None;
        for y in 0..=h {
            let edge = y < h && at(x - 1, y) != at(x, y);
            match (edge, run) {
                (true, None) => run = Some(y),
                (false, Some(start)) => {
                    edges.push(OutlineEdge {
                        x0: rect.x + x,
                        y0: rect.y + start,
                        x1: rect.x + x,
                        y1: rect.y + y,
                    });
                    run = None;
                }
                _ => {}
            }
        }
    }
    edges
}

/// Flip or turn a mask the same way [`transform_pixels`] turns the pixels.
pub(crate) fn transform_mask(
    width: u32,
    height: u32,
    mask: &[bool],
    op: Transform,
) -> Option<Vec<bool>> {
    let on = Rgba::new(255, 255, 255, 255);
    let encoded: Vec<Rgba> = mask
        .iter()
        .map(|m| if *m { on } else { Rgba::CLEAR })
        .collect();
    let (_, _, turned) = transform_pixels(width, height, &encoded, op)?;
    Some(turned.into_iter().map(|px| px.a != 0).collect())
}

/// The part of a mask that falls inside `keep`, a rectangle within `rect`.
pub(crate) fn crop_mask(rect: PixelRect, mask: &[bool], keep: PixelRect) -> Vec<bool> {
    let mut out = Vec::with_capacity(keep.width as usize * keep.height as usize);
    for y in keep.y..keep.y + keep.height as i32 {
        for x in keep.x..keep.x + keep.width as i32 {
            out.push(mask_contains(rect, Some(mask), x, y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> PixelRect {
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn normalize_trims_and_drops_a_full_mask() {
        // . X .
        // . X X
        let mask = vec![false, true, false, false, true, true];
        let (r, m) = normalize(rect(10, 10, 3, 2), Some(mask)).unwrap();
        assert_eq!(r, rect(11, 10, 2, 2));
        assert_eq!(m, Some(vec![true, false, true, true]));
        let (r, m) = normalize(rect(0, 0, 2, 1), Some(vec![true, true])).unwrap();
        assert_eq!((r, m), (rect(0, 0, 2, 1), None));
        assert!(normalize(rect(0, 0, 2, 1), Some(vec![false, false])).is_none());
        assert!(normalize(rect(0, 0, 2, 1), Some(vec![true])).is_none());
        assert_eq!(
            normalize(rect(3, 4, 5, 6), None),
            Some((rect(3, 4, 5, 6), None))
        );
    }

    #[test]
    fn combine_adds_and_subtracts_on_the_canvas() {
        let left = vec![true, true, false, false];
        let right = vec![false, false, true, true];
        let added = combine(
            Some((rect(0, 0, 2, 1), None)),
            right.clone(),
            SelectMode::Add,
            4,
            1,
        );
        assert_eq!(added, Some((rect(0, 0, 4, 1), None)));
        let gap = combine(
            Some((rect(0, 0, 4, 1), None)),
            vec![false, true, true, false],
            SelectMode::Subtract,
            4,
            1,
        );
        assert_eq!(
            gap,
            Some((rect(0, 0, 4, 1), Some(vec![true, false, false, true])))
        );
        assert_eq!(
            combine(
                Some((rect(0, 0, 2, 1), None)),
                left.clone(),
                SelectMode::Subtract,
                4,
                1
            ),
            None
        );
        assert_eq!(
            combine(None, left.clone(), SelectMode::Subtract, 4, 1),
            None
        );
        assert_eq!(
            combine(None, right.clone(), SelectMode::Add, 4, 1),
            Some((rect(2, 0, 2, 1), None))
        );
        assert_eq!(
            combine(
                Some((rect(0, 0, 2, 1), None)),
                right,
                SelectMode::Replace,
                4,
                1
            ),
            Some((rect(2, 0, 2, 1), None))
        );
        // A selection hanging off the canvas keeps only its on-canvas part when combined.
        assert_eq!(
            combine(
                Some((rect(-5, 0, 6, 1), None)),
                vec![false, false, false, true],
                SelectMode::Add,
                4,
                1
            ),
            Some((rect(0, 0, 4, 1), Some(vec![true, false, false, true])))
        );
    }

    #[test]
    fn outline_traces_a_rectangle_and_a_notch() {
        let edges = outline(rect(2, 3, 2, 1), None);
        assert_eq!(edges.len(), 4);
        assert!(edges.contains(&OutlineEdge {
            x0: 2,
            y0: 3,
            x1: 4,
            y1: 3
        }));
        assert!(edges.contains(&OutlineEdge {
            x0: 4,
            y0: 3,
            x1: 4,
            y1: 4
        }));
        // X X
        // X .
        let notch = outline(rect(0, 0, 2, 2), Some(&[true, true, true, false]));
        let length: i32 = notch.iter().map(|e| (e.x1 - e.x0) + (e.y1 - e.y0)).sum();
        assert_eq!(length, 8);
        assert!(notch.contains(&OutlineEdge {
            x0: 1,
            y0: 1,
            x1: 2,
            y1: 1
        }));
        assert!(notch.contains(&OutlineEdge {
            x0: 1,
            y0: 1,
            x1: 1,
            y1: 2
        }));
    }

    #[test]
    fn transform_and_crop_follow_the_pixels() {
        // X .
        // X X  turned clockwise is  X X / X .
        let mask = [true, false, true, true];
        assert_eq!(
            transform_mask(2, 2, &mask, Transform::Rotate90Cw),
            Some(vec![true, true, true, false])
        );
        assert_eq!(
            transform_mask(2, 2, &mask, Transform::FlipHorizontal),
            Some(vec![false, true, true, true])
        );
        assert_eq!(
            crop_mask(rect(0, 0, 2, 2), &mask, rect(0, 1, 2, 1)),
            vec![true, true]
        );
    }
}
