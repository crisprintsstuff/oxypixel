//! Pixel geometry: rectangles, lines, ellipses, brushes, and flood fill.

use crate::color::Rgba;

/// Inclusive rectangle in pixel coordinates. `width` and `height` are at least 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn from_corners(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        let left = i64::from(x0.min(x1));
        let right = i64::from(x0.max(x1));
        let top = i64::from(y0.min(y1));
        let bottom = i64::from(y0.max(y1));
        let width = (right - left + 1).clamp(1, 100_000) as u32;
        let height = (bottom - top + 1).clamp(1, 100_000) as u32;
        Self {
            x: left as i32,
            y: top as i32,
            width,
            height,
        }
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        let right = self.x.saturating_add(self.width as i32);
        let bottom = self.y.saturating_add(self.height as i32);
        x >= self.x && y >= self.y && x < right && y < bottom
    }
}

pub(crate) fn clamp_to_margin(v: i32, limit: i32, margin: i32) -> i32 {
    v.clamp(-margin, limit.saturating_add(margin))
}

/// Square brush offsets. The cursor pixel is inside the stamp.
/// Even sizes extend up and left of the cursor.
pub(crate) fn brush_offsets(size: u32) -> Vec<(i32, i32)> {
    let size = size.clamp(1, crate::limits::MAX_BRUSH) as i32;
    let half = size / 2;
    let mut offsets = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            offsets.push((x - half, y - half));
        }
    }
    offsets
}

pub(crate) fn line_points(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let mut points = Vec::new();
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;
    loop {
        points.push((x, y));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        if points.len() > 8_000 {
            break;
        }
    }
    points
}

/// Snap the end point to the nearest 45-degree direction from the origin.
pub(crate) fn constrain_line(x0: i32, y0: i32, x1: i32, y1: i32) -> (i32, i32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    if dx == 0 && dy == 0 {
        return (x1, y1);
    }
    let angle = (dy as f32).atan2(dx as f32);
    let step = std::f32::consts::FRAC_PI_4;
    let snapped = (angle / step).round() * step;
    let len = ((i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)) as f32).sqrt();
    let nx = x0 + (snapped.cos() * len).round() as i32;
    let ny = y0 + (snapped.sin() * len).round() as i32;
    (nx, ny)
}

/// Snap the end point so the span is a square.
pub(crate) fn constrain_square(x0: i32, y0: i32, x1: i32, y1: i32) -> (i32, i32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let side = dx.abs().max(dy.abs());
    let sx = if dx < 0 { -side } else { side };
    let sy = if dy < 0 { -side } else { side };
    (x0 + sx, y0 + sy)
}

pub(crate) fn rect_points(x0: i32, y0: i32, x1: i32, y1: i32, filled: bool) -> Vec<(i32, i32)> {
    let rect = PixelRect::from_corners(x0, y0, x1, y1);
    let mut points = Vec::new();
    let right = rect.x + rect.width as i32 - 1;
    let bottom = rect.y + rect.height as i32 - 1;
    if filled {
        for y in rect.y..=bottom {
            for x in rect.x..=right {
                points.push((x, y));
            }
        }
        return points;
    }
    for x in rect.x..=right {
        points.push((x, rect.y));
        if bottom != rect.y {
            points.push((x, bottom));
        }
    }
    for y in (rect.y + 1)..bottom {
        points.push((rect.x, y));
        if right != rect.x {
            points.push((right, y));
        }
    }
    points
}

fn ellipse_mask(rect: PixelRect) -> Vec<(i32, i32)> {
    if rect.width == 1 && rect.height == 1 {
        return vec![(rect.x, rect.y)];
    }
    let rx = (rect.width as f32 - 1.0) / 2.0;
    let ry = (rect.height as f32 - 1.0) / 2.0;
    let cx = rect.x as f32 + rx;
    let cy = rect.y as f32 + ry;
    let right = rect.x + rect.width as i32;
    let bottom = rect.y + rect.height as i32;
    let mut inside = Vec::new();
    for y in rect.y..bottom {
        for x in rect.x..right {
            if rx == 0.0 || ry == 0.0 {
                inside.push((x, y));
                continue;
            }
            let nx = (x as f32 - cx) / rx;
            let ny = (y as f32 - cy) / ry;
            if nx * nx + ny * ny <= 1.0 + 1e-4 {
                inside.push((x, y));
            }
        }
    }
    inside
}

pub(crate) fn ellipse_points(x0: i32, y0: i32, x1: i32, y1: i32, filled: bool) -> Vec<(i32, i32)> {
    let rect = PixelRect::from_corners(x0, y0, x1, y1);
    let inside = ellipse_mask(rect);
    if filled || rect.width == 1 || rect.height == 1 {
        return inside;
    }
    let set: std::collections::HashSet<(i32, i32)> = inside.iter().copied().collect();
    inside
        .into_iter()
        .filter(|(x, y)| {
            [(*x + 1, *y), (*x - 1, *y), (*x, *y + 1), (*x, *y - 1)]
                .into_iter()
                .any(|p| !set.contains(&p))
        })
        .collect()
}

/// Four-connected fill on one cel. Pixels outside `selection` are walls.
/// Returns whether any pixel changed.
pub(crate) fn flood_fill(
    pixels: &mut [Rgba],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    color: Rgba,
    selection: Option<PixelRect>,
) -> bool {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
        return false;
    }
    if selection.is_some_and(|sel| !sel.contains(x, y)) {
        return false;
    }
    let start = (y as u32 * width + x as u32) as usize;
    let target = pixels[start];
    if target == color {
        return false;
    }
    let mut stack = vec![(x, y)];
    let mut seen = vec![false; pixels.len()];
    let mut changed = false;
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx as u32 >= width || cy as u32 >= height {
            continue;
        }
        if selection.is_some_and(|sel| !sel.contains(cx, cy)) {
            continue;
        }
        let i = (cy as u32 * width + cx as u32) as usize;
        if seen[i] || pixels[i] != target {
            continue;
        }
        seen[i] = true;
        pixels[i] = color;
        changed = true;
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_includes_endpoints() {
        assert_eq!(
            line_points(0, 0, 3, 0),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        assert_eq!(line_points(1, 1, 1, 1), vec![(1, 1)]);
        let diag = line_points(0, 0, 2, 2);
        assert_eq!(diag.first(), Some(&(0, 0)));
        assert_eq!(diag.last(), Some(&(2, 2)));
        assert!(diag.len() >= 3);
    }

    #[test]
    fn constraints_snap() {
        assert_eq!(constrain_line(0, 0, 10, 1), (10, 0));
        assert_eq!(constrain_line(0, 0, 10, 10), (10, 10));
        assert_eq!(constrain_square(0, 0, 10, 3), (10, 10));
        assert_eq!(constrain_square(5, 5, 1, 4), (1, 1));
    }

    #[test]
    fn rect_and_ellipse() {
        let outline = rect_points(0, 0, 2, 2, false);
        assert_eq!(outline.len(), 8);
        assert!(!outline.contains(&(1, 1)));
        assert_eq!(rect_points(0, 0, 2, 2, true).len(), 9);

        let filled = ellipse_points(0, 0, 6, 6, true);
        assert!(filled.contains(&(3, 3)));
        assert!(filled.contains(&(0, 3)));
        assert!(!filled.contains(&(0, 0)));
        let edge = ellipse_points(0, 0, 6, 6, false);
        assert!(!edge.contains(&(3, 3)));
        assert!(edge.contains(&(0, 3)));
        assert_eq!(ellipse_points(4, 4, 4, 4, false), vec![(4, 4)]);
    }

    #[test]
    fn brush_size_one_is_the_cursor() {
        assert_eq!(brush_offsets(1), vec![(0, 0)]);
        assert_eq!(brush_offsets(2).len(), 4);
    }

    #[test]
    fn flood_fill_stops_at_a_different_color_and_selection() {
        let mut pixels = vec![Rgba::CLEAR; 9];
        pixels[1] = Rgba::new(1, 0, 0, 255);
        assert!(flood_fill(
            &mut pixels,
            3,
            3,
            0,
            0,
            Rgba::new(0, 0, 2, 255),
            None
        ));
        assert_eq!(pixels[0], Rgba::new(0, 0, 2, 255));
        assert_eq!(pixels[1], Rgba::new(1, 0, 0, 255));
        assert_eq!(pixels[3].a, 255);

        let mut pixels = vec![Rgba::CLEAR; 9];
        let sel = PixelRect::from_corners(0, 0, 0, 2);
        flood_fill(&mut pixels, 3, 3, 0, 0, Rgba::new(9, 0, 0, 255), Some(sel));
        assert_eq!(pixels[0].r, 9);
        assert_eq!(pixels[1], Rgba::CLEAR);
    }
}
