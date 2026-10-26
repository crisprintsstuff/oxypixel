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

/// Flip or turn a rectangle of pixels.
///
/// A 90 degree turn swaps width and height. See [`turned_rect`] for where that
/// rectangle sits on the cel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    FlipHorizontal,
    FlipVertical,
    Rotate90Cw,
    Rotate90Ccw,
    Rotate180,
}

/// Map `pixels` (row-major, `width` by `height`) through `op`.
///
/// Returns the new width, the new height, and the new pixels. A zero side, a
/// buffer whose length is not `width * height`, or a buffer larger than one cel
/// returns `None` and allocates nothing.
pub(crate) fn transform_pixels(
    width: u32,
    height: u32,
    pixels: &[Rgba],
    op: Transform,
) -> Option<(u32, u32, Vec<Rgba>)> {
    if width == 0 || height == 0 {
        return None;
    }
    let len = (width as usize).checked_mul(height as usize)?;
    if len > crate::limits::MAX_CEL_PIXELS as usize || pixels.len() != len {
        return None;
    }
    let (out_w, out_h) = match op {
        Transform::Rotate90Cw | Transform::Rotate90Ccw => (height, width),
        Transform::FlipHorizontal | Transform::FlipVertical | Transform::Rotate180 => {
            (width, height)
        }
    };
    let mut out = vec![Rgba::CLEAR; len];
    for y in 0..height {
        for x in 0..width {
            let (nx, ny) = match op {
                Transform::FlipHorizontal => (width - 1 - x, y),
                Transform::FlipVertical => (x, height - 1 - y),
                Transform::Rotate180 => (width - 1 - x, height - 1 - y),
                Transform::Rotate90Cw => (height - 1 - y, x),
                Transform::Rotate90Ccw => (y, width - 1 - x),
            };
            let src = (y as usize)
                .checked_mul(width as usize)?
                .checked_add(x as usize)?;
            let dst = (ny as usize)
                .checked_mul(out_w as usize)?
                .checked_add(nx as usize)?;
            let px = pixels.get(src).copied()?;
            *out.get_mut(dst)? = px;
        }
    }
    Some((out_w, out_h, out))
}

/// Where a transformed rectangle lands.
///
/// Flips and a 180 degree turn keep `src`. A 90 degree turn swaps the sides and
/// keeps the same center, using integer division so an odd leftover sits toward
/// the top and left. When that turned rectangle fits on the cel and still
/// touches it, it shifts by the smallest amount that puts it fully inside. A
/// rectangle that does not touch the cel stays where the center put it. When
/// the turned rectangle is wider or taller than the cel, the centered position
/// is left alone and the caller drops the pixels that fall outside.
pub(crate) fn turned_rect(
    src: PixelRect,
    canvas_w: u32,
    canvas_h: u32,
    op: Transform,
) -> PixelRect {
    let (width, height) = match op {
        Transform::Rotate90Cw | Transform::Rotate90Ccw => (src.height, src.width),
        Transform::FlipHorizontal | Transform::FlipVertical | Transform::Rotate180 => {
            (src.width, src.height)
        }
    };
    if !matches!(op, Transform::Rotate90Cw | Transform::Rotate90Ccw) {
        return src;
    }
    let mut x = i64::from(src.x) + (i64::from(src.width) - i64::from(width)) / 2;
    let mut y = i64::from(src.y) + (i64::from(src.height) - i64::from(height)) / 2;
    let span_w = i64::from(width);
    let span_h = i64::from(height);
    let limit_w = i64::from(canvas_w);
    let limit_h = i64::from(canvas_h);
    let fits = span_w <= limit_w && span_h <= limit_h;
    let touches = x < limit_w && y < limit_h && x + span_w > 0 && y + span_h > 0;
    if fits && touches {
        if x < 0 {
            x = 0;
        } else if x + span_w > limit_w {
            x = limit_w - span_w;
        }
        if y < 0 {
            y = 0;
        } else if y + span_h > limit_h {
            y = limit_h - span_h;
        }
    }
    PixelRect {
        x: clamp_i64_to_i32(x),
        y: clamp_i64_to_i32(y),
        width,
        height,
    }
}

/// The part of `rect` that lies on a canvas of `canvas_w` by `canvas_h`.
pub(crate) fn rect_on_canvas(rect: PixelRect, canvas_w: u32, canvas_h: u32) -> Option<PixelRect> {
    let left = i64::from(rect.x);
    let top = i64::from(rect.y);
    let right = left.saturating_add(i64::from(rect.width));
    let bottom = top.saturating_add(i64::from(rect.height));
    let x0 = left.max(0);
    let y0 = top.max(0);
    let x1 = right.min(i64::from(canvas_w));
    let y1 = bottom.min(i64::from(canvas_h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(PixelRect {
        x: clamp_i64_to_i32(x0),
        y: clamp_i64_to_i32(y0),
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    })
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    if value > i64::from(i32::MAX) {
        i32::MAX
    } else if value < i64::from(i32::MIN) {
        i32::MIN
    } else {
        value as i32
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

/// Fold `value` into `0..size`. Negatives come in from the far side.
///
/// `size` of 0 or less returns 0. `i32::MIN` is safe: the remainder is computed
/// in `i64`, so it does not overflow.
pub(crate) fn wrap_coord(value: i32, size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    let size = i64::from(size);
    let mut wrapped = i64::from(value) % size;
    if wrapped < 0 {
        wrapped += size;
    }
    wrapped as i32
}

/// `b` is the extra corner of a one-pixel diagonal step from `a` to `c`.
///
/// `a` and `c` are diagonal neighbors. `b` is one of the two orthogonal pixels
/// between them. A freehand stroke that visits `a`, then `b`, then `c` draws an L.
pub(crate) fn is_doubled_corner(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> bool {
    let dx = c.0.wrapping_sub(a.0);
    let dy = c.1.wrapping_sub(a.1);
    if dx.abs() != 1 || dy.abs() != 1 {
        return false;
    }
    (b.0 == a.0 && b.1 == c.1) || (b.1 == a.1 && b.0 == c.0)
}

/// Drop doubled corners from a one-pixel freehand path. Other points stay, in order.
///
/// A repeated point is ignored. A straight run is unchanged. The line tool does
/// not use this.
pub(crate) fn pixel_perfect_path(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut kept = Vec::with_capacity(points.len());
    for &point in points {
        if kept.last() == Some(&point) {
            continue;
        }
        if kept.len() >= 2 {
            let a = kept[kept.len() - 2];
            let b = kept[kept.len() - 1];
            if is_doubled_corner(a, b, point) {
                kept.pop();
            }
        }
        if kept.last() != Some(&point) {
            kept.push(point);
        }
    }
    kept
}

/// Where a fill is allowed to spread, and which pixels count as the same color.
#[derive(Clone, Copy, Default)]
pub(crate) struct FillLimits<'a> {
    pub selection: Option<PixelRect>,
    /// Which pixels of `selection` are selected, row by row. `None` is all of them.
    pub mask: Option<&'a [bool]>,
    pub wrap: bool,
    /// Largest per-channel RGBA difference from the start pixel that still
    /// fills. 0 is an exact match.
    pub tolerance: u8,
    /// Fill every matching pixel on the cel, not only the connected ones.
    pub global: bool,
}

impl FillLimits<'_> {
    fn allows(&self, x: i32, y: i32) -> bool {
        self.selection
            .is_none_or(|sel| crate::selection::mask_contains(sel, self.mask, x, y))
    }
}

/// Whether `pixel` is within `tolerance` of `target` on every RGBA channel.
pub(crate) fn fill_matches(target: Rgba, pixel: Rgba, tolerance: u8) -> bool {
    let near = |a: u8, b: u8| a.abs_diff(b) <= tolerance;
    near(target.r, pixel.r)
        && near(target.g, pixel.g)
        && near(target.b, pixel.b)
        && near(target.a, pixel.a)
}

/// The pixels a fill from `(x, y)` would reach, one `bool` per cel pixel.
/// `None` when the start is off the cel or outside the selection.
///
/// Pixels outside `limits.selection` are walls. When `limits.wrap` is set, a
/// step past an edge continues on the opposite edge. `limits.tolerance` widens
/// what counts as the start color, and `limits.global` takes every match on
/// the cel instead of a connected area.
pub(crate) fn fill_region(
    pixels: &[Rgba],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    limits: FillLimits,
) -> Option<Vec<bool>> {
    let len = (width as usize).checked_mul(height as usize)?;
    if width == 0 || height == 0 || pixels.len() != len {
        return None;
    }
    let (x, y) = if limits.wrap {
        (wrap_coord(x, width as i32), wrap_coord(y, height as i32))
    } else {
        (x, y)
    };
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height || !limits.allows(x, y) {
        return None;
    }
    let target = pixels[(y as u32 * width + x as u32) as usize];
    let tolerance = limits.tolerance;
    let mut region = vec![false; len];
    if limits.global {
        for (i, pixel) in pixels.iter().enumerate() {
            let px = (i % width as usize) as i32;
            let py = (i / width as usize) as i32;
            region[i] = limits.allows(px, py) && fill_matches(target, *pixel, tolerance);
        }
        return Some(region);
    }
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx as u32 >= width || cy as u32 >= height {
            continue;
        }
        if !limits.allows(cx, cy) {
            continue;
        }
        let i = (cy as u32 * width + cx as u32) as usize;
        if region[i] || !fill_matches(target, pixels[i], tolerance) {
            continue;
        }
        region[i] = true;
        let wrap = limits.wrap;
        push_fill_neighbor(&mut stack, cx + 1, cy, width, height, wrap);
        push_fill_neighbor(&mut stack, cx - 1, cy, width, height, wrap);
        push_fill_neighbor(&mut stack, cx, cy + 1, width, height, wrap);
        push_fill_neighbor(&mut stack, cx, cy - 1, width, height, wrap);
    }
    Some(region)
}

/// Four-connected fill on one cel, over the pixels [`fill_region`] reaches.
/// Returns whether any pixel changed.
pub(crate) fn flood_fill(
    pixels: &mut [Rgba],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    color: Rgba,
    limits: FillLimits,
) -> bool {
    let Some(region) = fill_region(pixels, width, height, x, y, limits) else {
        return false;
    };
    let mut changed = false;
    for (pixel, inside) in pixels.iter_mut().zip(region) {
        if inside && *pixel != color {
            *pixel = color;
            changed = true;
        }
    }
    changed
}

fn push_fill_neighbor(
    stack: &mut Vec<(i32, i32)>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    wrap: bool,
) {
    let (x, y) = if wrap {
        (wrap_coord(x, width as i32), wrap_coord(y, height as i32))
    } else {
        (x, y)
    };
    stack.push((x, y));
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
            FillLimits {
                selection: None,
                wrap: false,
                ..Default::default()
            },
        ));
        assert_eq!(pixels[0], Rgba::new(0, 0, 2, 255));
        assert_eq!(pixels[1], Rgba::new(1, 0, 0, 255));
        assert_eq!(pixels[3].a, 255);

        let mut pixels = vec![Rgba::CLEAR; 9];
        let sel = PixelRect::from_corners(0, 0, 0, 2);
        flood_fill(
            &mut pixels,
            3,
            3,
            0,
            0,
            Rgba::new(9, 0, 0, 255),
            FillLimits {
                selection: Some(sel),
                wrap: false,
                ..Default::default()
            },
        );
        assert_eq!(pixels[0].r, 9);
        assert_eq!(pixels[1], Rgba::CLEAR);
    }

    fn solid(n: u8) -> Rgba {
        Rgba::new(n, 0, 0, 255)
    }

    #[test]
    fn flips_and_half_turns_stay_in_the_same_rectangle() {
        let pixels = [solid(1), solid(2), solid(3), solid(4)];
        let (w, h, flipped) = transform_pixels(2, 2, &pixels, Transform::FlipHorizontal).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(flipped, vec![solid(2), solid(1), solid(4), solid(3)]);
        let (_, _, vertical) = transform_pixels(2, 2, &pixels, Transform::FlipVertical).unwrap();
        assert_eq!(vertical, vec![solid(3), solid(4), solid(1), solid(2)]);
        let (_, _, half) = transform_pixels(2, 2, &pixels, Transform::Rotate180).unwrap();
        assert_eq!(half, vec![solid(4), solid(3), solid(2), solid(1)]);
        let src = PixelRect {
            x: 2,
            y: 3,
            width: 2,
            height: 2,
        };
        assert_eq!(turned_rect(src, 8, 8, Transform::FlipHorizontal), src);
        assert_eq!(turned_rect(src, 8, 8, Transform::Rotate180), src);
    }

    #[test]
    fn quarter_turns_swap_the_sides_and_round_trip() {
        let row = [solid(1), solid(2)];
        let (w, h, cw) = transform_pixels(2, 1, &row, Transform::Rotate90Cw).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(cw, vec![solid(1), solid(2)]);
        let (w, h, ccw) = transform_pixels(2, 1, &row, Transform::Rotate90Ccw).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(ccw, vec![solid(2), solid(1)]);

        let block = [solid(1), solid(2), solid(3), solid(4), solid(5), solid(6)];
        let (_, _, turned) = transform_pixels(3, 2, &block, Transform::Rotate90Cw).unwrap();
        assert_eq!(
            turned,
            vec![solid(4), solid(1), solid(5), solid(2), solid(6), solid(3)]
        );
        let (_, _, back) = transform_pixels(2, 3, &turned, Transform::Rotate90Ccw).unwrap();
        assert_eq!(back, block);

        let mut spinning = block.to_vec();
        let mut w = 3;
        let mut h = 2;
        for _ in 0..4 {
            let (nw, nh, next) = transform_pixels(w, h, &spinning, Transform::Rotate90Cw).unwrap();
            spinning = next;
            w = nw;
            h = nh;
        }
        assert_eq!((w, h), (3, 2));
        assert_eq!(spinning, block);
    }

    #[test]
    fn a_wide_turn_shifts_onto_the_canvas_and_a_taller_one_stays_centered() {
        let wide = PixelRect {
            x: 0,
            y: 0,
            width: 2,
            height: 4,
        };
        assert_eq!(
            turned_rect(wide, 8, 8, Transform::Rotate90Cw),
            PixelRect {
                x: 0,
                y: 1,
                width: 4,
                height: 2,
            }
        );
        let cel = PixelRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        };
        let turned = turned_rect(cel, 3, 1, Transform::Rotate90Cw);
        assert_eq!(
            turned,
            PixelRect {
                x: 1,
                y: -1,
                width: 1,
                height: 3,
            }
        );
        assert_eq!(
            rect_on_canvas(turned, 3, 1),
            Some(PixelRect {
                x: 1,
                y: 0,
                width: 1,
                height: 1,
            })
        );
        assert_eq!(
            rect_on_canvas(
                PixelRect {
                    x: 100,
                    y: 100,
                    width: 1,
                    height: 1,
                },
                8,
                8
            ),
            None
        );
        let away = PixelRect {
            x: 100,
            y: 100,
            width: 2,
            height: 1,
        };
        let parked = turned_rect(away, 8, 8, Transform::Rotate90Cw);
        assert!(parked.x >= 8 || parked.y >= 8);
        assert_eq!(rect_on_canvas(parked, 8, 8), None);
    }

    #[test]
    fn a_bad_buffer_is_refused_before_allocation() {
        assert!(transform_pixels(0, 2, &[], Transform::FlipHorizontal).is_none());
        assert!(transform_pixels(2, 2, &[solid(1)], Transform::Rotate90Cw).is_none());
    }

    #[test]
    fn pixel_perfect_drops_only_the_doubled_corner() {
        assert!(is_doubled_corner((0, 0), (1, 0), (1, 1)));
        assert!(is_doubled_corner((0, 0), (0, 1), (1, 1)));
        assert!(!is_doubled_corner((0, 0), (1, 0), (2, 0)));
        assert!(!is_doubled_corner((0, 0), (1, 0), (2, 1)));

        assert_eq!(
            pixel_perfect_path(&[(0, 0), (1, 0), (1, 1)]),
            vec![(0, 0), (1, 1)]
        );
        assert_eq!(
            pixel_perfect_path(&[(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)]),
            vec![(0, 0), (1, 1), (2, 2)]
        );
        assert_eq!(
            pixel_perfect_path(&[(0, 0), (1, 0), (2, 0), (3, 0)]),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        assert_eq!(
            pixel_perfect_path(&[(4, 4), (4, 4), (5, 5)]),
            vec![(4, 4), (5, 5)]
        );
    }

    #[test]
    fn wrap_coord_folds_both_edges_and_min_does_not_panic() {
        assert_eq!(wrap_coord(0, 8), 0);
        assert_eq!(wrap_coord(8, 8), 0);
        assert_eq!(wrap_coord(9, 8), 1);
        assert_eq!(wrap_coord(-1, 8), 7);
        assert_eq!(wrap_coord(-8, 8), 0);
        assert_eq!(wrap_coord(-9, 8), 7);
        assert_eq!(wrap_coord(3, 1), 0);
        assert_eq!(wrap_coord(-4, 0), 0);
        let wrapped = wrap_coord(i32::MIN, 8);
        assert!((0..8).contains(&wrapped));
        assert_eq!(wrap_coord(i32::MAX, 8), (i32::MAX as i64 % 8) as i32);
    }

    #[test]
    fn fill_tolerance_takes_near_colors_and_stops_at_far_ones() {
        let base = Rgba::new(100, 100, 100, 255);
        let near = Rgba::new(108, 95, 100, 255);
        let far = Rgba::new(140, 100, 100, 255);
        let ink = Rgba::new(0, 0, 0, 255);
        assert!(fill_matches(base, base, 0));
        assert!(!fill_matches(base, near, 0));
        assert!(fill_matches(base, near, 8));
        assert!(!fill_matches(base, near, 7));
        assert!(!fill_matches(base, Rgba::new(100, 100, 100, 250), 4));

        // Row: base near far base. Tolerance 10 crosses `near` and stops at `far`.
        let row = [base, near, far, base];
        let mut pixels = row.to_vec();
        let limits = FillLimits {
            tolerance: 10,
            ..Default::default()
        };
        assert!(flood_fill(&mut pixels, 4, 1, 0, 0, ink, limits));
        assert_eq!(pixels, vec![ink, ink, far, base]);

        // Tolerance 0 is the exact fill from before.
        let mut exact = row.to_vec();
        assert!(flood_fill(
            &mut exact,
            4,
            1,
            0,
            0,
            ink,
            FillLimits::default()
        ));
        assert_eq!(exact, vec![ink, near, far, base]);

        // Clicking a pixel already in the ink color still fills its near neighbors.
        let mut same = vec![ink, Rgba::new(3, 3, 3, 255), far];
        assert!(flood_fill(&mut same, 3, 1, 0, 0, ink, limits));
        assert_eq!(same, vec![ink, ink, far]);
        let mut nothing = vec![ink, far];
        assert!(!flood_fill(&mut nothing, 2, 1, 0, 0, ink, limits));
    }

    #[test]
    fn global_fill_reaches_disconnected_matches_inside_the_selection() {
        let base = Rgba::new(10, 10, 10, 255);
        let wall = Rgba::new(200, 0, 0, 255);
        let ink = Rgba::new(0, 0, 255, 255);
        // base wall base / wall wall base: the right column is cut off.
        let start = vec![base, wall, base, wall, wall, base];
        let mut pixels = start.clone();
        let global = FillLimits {
            global: true,
            ..Default::default()
        };
        assert!(flood_fill(&mut pixels, 3, 2, 0, 0, ink, global));
        assert_eq!(pixels, vec![ink, wall, ink, wall, wall, ink]);

        let mut contiguous = start.clone();
        assert!(flood_fill(
            &mut contiguous,
            3,
            2,
            0,
            0,
            ink,
            FillLimits::default()
        ));
        assert_eq!(contiguous, vec![ink, wall, base, wall, wall, base]);

        // The selection still bounds a global fill.
        let mut selected = start.clone();
        let limits = FillLimits {
            selection: Some(PixelRect::from_corners(0, 0, 1, 1)),
            global: true,
            ..Default::default()
        };
        assert!(flood_fill(&mut selected, 3, 2, 0, 0, ink, limits));
        assert_eq!(selected, vec![ink, wall, base, wall, wall, base]);

        // A global fill with tolerance takes near colors anywhere.
        let mut near = vec![base, wall, Rgba::new(14, 10, 6, 255)];
        let limits = FillLimits {
            global: true,
            tolerance: 4,
            ..Default::default()
        };
        assert!(flood_fill(&mut near, 3, 1, 0, 0, ink, limits));
        assert_eq!(near, vec![ink, wall, ink]);
    }

    #[test]
    fn flood_fill_wraps_around_a_wall_on_the_near_side() {
        let wall = Rgba::new(1, 0, 0, 255);
        let ink = Rgba::new(0, 0, 9, 255);
        let mut pixels = vec![Rgba::CLEAR, wall, Rgba::CLEAR, Rgba::CLEAR];
        assert!(flood_fill(
            &mut pixels,
            4,
            1,
            0,
            0,
            ink,
            FillLimits {
                selection: None,
                wrap: true,
                ..Default::default()
            },
        ));
        assert_eq!(pixels[0], ink);
        assert_eq!(pixels[1], wall);
        assert_eq!(pixels[2], ink);
        assert_eq!(pixels[3], ink);

        let mut blocked = vec![Rgba::CLEAR, wall, Rgba::CLEAR, Rgba::CLEAR];
        assert!(flood_fill(
            &mut blocked,
            4,
            1,
            0,
            0,
            ink,
            FillLimits {
                selection: None,
                wrap: false,
                ..Default::default()
            },
        ));
        assert_eq!(blocked[0], ink);
        assert_eq!(blocked[3], Rgba::CLEAR);
    }
}
