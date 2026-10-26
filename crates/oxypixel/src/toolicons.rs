//! Original 16×16 tool icons, drawn in code.
//!
//! The colors are the same amber and teal as the window theme. These pictures
//! are not copied from another editor.

use oxypixel_core::Tool;

pub const SIZE: usize = 16;

const AMBER: [u8; 4] = [232, 168, 56, 255];
const TEAL: [u8; 4] = [58, 161, 122, 255];
const PAPER: [u8; 4] = [246, 243, 236, 255];

struct Pix {
    px: [u8; SIZE * SIZE * 4],
}

impl Pix {
    fn new() -> Self {
        Self {
            px: [0; SIZE * SIZE * 4],
        }
    }

    fn put(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
            return;
        }
        let i = (y as usize * SIZE + x as usize) * 4;
        self.px[i..i + 4].copy_from_slice(&color);
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;
        for _ in 0..64 {
            self.put(x, y, color);
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
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, color);
            }
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.line(x, y, x + w - 1, y, color);
        self.line(x, y, x, y + h - 1, color);
        self.line(x + w - 1, y, x + w - 1, y + h - 1, color);
        self.line(x, y + h - 1, x + w - 1, y + h - 1, color);
    }

    fn circle(&mut self, cx: i32, cy: i32, radius: i32, color: [u8; 4]) {
        let mut x = radius;
        let mut y = 0;
        let mut err = 1 - radius;
        while x >= y {
            self.put(cx + x, cy + y, color);
            self.put(cx + y, cy + x, color);
            self.put(cx - y, cy + x, color);
            self.put(cx - x, cy + y, color);
            self.put(cx - x, cy - y, color);
            self.put(cx - y, cy - x, color);
            self.put(cx + y, cy - x, color);
            self.put(cx + x, cy - y, color);
            y += 1;
            if err < 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }

    fn dashed_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
        for x in x0..=x1 {
            if (x - x0) % 2 == 0 {
                self.put(x, y0, color);
                self.put(x, y1, color);
            }
        }
        for y in y0..=y1 {
            if (y - y0) % 2 == 0 {
                self.put(x0, y, color);
                self.put(x1, y, color);
            }
        }
    }
}

pub fn rgba(tool: Tool) -> Vec<u8> {
    let mut pix = Pix::new();
    match tool {
        Tool::Pencil => {
            pix.line(3, 12, 11, 4, AMBER);
            pix.put(12, 3, TEAL);
            pix.put(11, 3, TEAL);
            pix.put(12, 4, AMBER);
            pix.fill_rect(2, 13, 2, 2, PAPER);
        }
        Tool::Eraser => {
            pix.fill_rect(4, 5, 8, 7, PAPER);
            pix.rect(4, 5, 8, 7, AMBER);
            pix.line(5, 8, 10, 8, TEAL);
        }
        Tool::Bucket => {
            pix.fill_rect(3, 7, 7, 6, TEAL);
            pix.line(9, 7, 12, 4, AMBER);
            pix.line(10, 7, 12, 5, AMBER);
            pix.put(6, 14, AMBER);
        }
        Tool::Eyedropper => {
            pix.line(3, 12, 9, 6, PAPER);
            pix.line(4, 12, 10, 6, AMBER);
            pix.fill_rect(10, 3, 3, 3, TEAL);
            pix.put(11, 2, TEAL);
        }
        Tool::Line => {
            pix.line(3, 12, 12, 3, AMBER);
            pix.put(3, 12, TEAL);
            pix.put(12, 3, TEAL);
        }
        Tool::Rectangle => {
            pix.rect(3, 3, 10, 10, AMBER);
            pix.put(3, 3, TEAL);
            pix.put(12, 3, TEAL);
            pix.put(3, 12, TEAL);
            pix.put(12, 12, TEAL);
        }
        Tool::Ellipse => {
            pix.circle(7, 7, 5, AMBER);
            pix.put(7, 2, TEAL);
            pix.put(7, 12, TEAL);
        }
        Tool::Select => {
            pix.dashed_rect(2, 2, 13, 13, AMBER);
            pix.fill_rect(2, 2, 2, 2, TEAL);
            pix.fill_rect(12, 2, 2, 2, TEAL);
            pix.fill_rect(2, 12, 2, 2, TEAL);
            pix.fill_rect(12, 12, 2, 2, TEAL);
        }
        Tool::Wand => {
            pix.line(2, 13, 9, 6, PAPER);
            pix.line(3, 13, 9, 7, AMBER);
            pix.put(11, 4, TEAL);
            pix.line(11, 1, 11, 3, TEAL);
            pix.line(11, 5, 11, 7, TEAL);
            pix.line(8, 4, 10, 4, TEAL);
            pix.line(12, 4, 14, 4, TEAL);
        }
    }
    pix.px.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_visible_icon() {
        for tool in [
            Tool::Pencil,
            Tool::Eraser,
            Tool::Bucket,
            Tool::Eyedropper,
            Tool::Line,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Select,
            Tool::Wand,
        ] {
            let px = rgba(tool);
            assert_eq!(px.len(), SIZE * SIZE * 4);
            assert!(px.chunks(4).any(|p| p[3] == 255));
        }
    }
}
