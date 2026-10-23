//! Straight-alpha RGBA and source-over compositing.

/// One pixel. Channels are straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const CLEAR: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#RRGGBB` or `#RRGGBBAA`. A leading `#` is optional.
    pub fn parse_hex(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('#');
        match text.len() {
            6 => {
                let n = u32::from_str_radix(text, 16).ok()?;
                Some(Self::new((n >> 16) as u8, (n >> 8) as u8, n as u8, 255))
            }
            8 => {
                let n = u32::from_str_radix(text, 16).ok()?;
                Some(Self::new(
                    (n >> 24) as u8,
                    (n >> 16) as u8,
                    (n >> 8) as u8,
                    n as u8,
                ))
            }
            _ => None,
        }
    }

    /// `#RRGGBB` when opaque, otherwise `#RRGGBBAA`.
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Scale alpha. `factor` is 0–255.
    pub fn fade(self, factor: u8) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: mul_div(u32::from(self.a), u32::from(factor)) as u8,
        }
    }
}

/// Round-to-nearest `(a * b) / 255`.
fn mul_div(a: u32, b: u32) -> u32 {
    (a * b + 127) / 255
}

/// Porter-Duff source-over. `opacity` scales the source alpha (layer opacity).
///
/// Fully transparent source leaves `dst` unchanged. Fully opaque source
/// replaces `dst`.
pub fn source_over(dst: Rgba, src: Rgba, opacity: u8) -> Rgba {
    if opacity == 0 || src.a == 0 {
        return dst;
    }
    let sa = mul_div(u32::from(src.a), u32::from(opacity));
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return Rgba::new(src.r, src.g, src.b, 255);
    }
    let inv = 255 - sa;
    let out_a = (sa + mul_div(u32::from(dst.a), inv)).min(255);
    if out_a == 0 {
        return Rgba::CLEAR;
    }
    let mix = |sc: u8, dc: u8| -> u8 {
        let src_p = u32::from(sc) * sa;
        let dst_p = mul_div(u32::from(dc) * u32::from(dst.a), inv);
        ((src_p + dst_p) / out_a) as u8
    };
    Rgba::new(
        mix(src.r, dst.r),
        mix(src.g, dst.g),
        mix(src.b, dst.b),
        out_a as u8,
    )
}

pub(crate) fn pack(pixels: &[Rgba]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 4);
    for px in pixels {
        out.extend_from_slice(&px.to_bytes());
    }
    out
}

/// Linear RGBA ramp of `steps` colors, including both ends.
///
/// `steps` must be from 2 through the palette cap. The first color is `start`
/// and the last is `end`. Channels round to the nearest value.
pub(crate) fn color_ramp(start: Rgba, end: Rgba, steps: usize) -> Option<Vec<Rgba>> {
    if !(2..=crate::limits::MAX_PALETTE).contains(&steps) {
        return None;
    }
    let span = steps - 1;
    let mut colors = Vec::with_capacity(steps);
    for i in 0..steps {
        colors.push(Rgba::new(
            lerp_channel(start.r, end.r, i, span),
            lerp_channel(start.g, end.g, i, span),
            lerp_channel(start.b, end.b, i, span),
            lerp_channel(start.a, end.a, i, span),
        ));
    }
    if let Some(first) = colors.first_mut() {
        *first = start;
    }
    if let Some(last) = colors.last_mut() {
        *last = end;
    }
    Some(colors)
}

/// Closest swatch in squared RGBA distance. Equal distances keep the earlier index.
/// An empty palette returns `color`.
pub(crate) fn nearest_palette_color(color: Rgba, palette: &[Rgba]) -> Rgba {
    let mut best = color;
    let mut best_distance = u32::MAX;
    let mut found = false;
    for swatch in palette {
        let distance = color_distance_sq(color, *swatch);
        if !found || distance < best_distance {
            best = *swatch;
            best_distance = distance;
            found = true;
        }
    }
    best
}

fn lerp_channel(start: u8, end: u8, index: usize, span: usize) -> u8 {
    if index == 0 {
        return start;
    }
    if index == span {
        return end;
    }
    let start = i32::from(start);
    let delta = i32::from(end) - start;
    let span = span as i32;
    let numer = delta * index as i32;
    let half = span / 2;
    let rounded = if numer >= 0 {
        numer + half
    } else {
        numer - half
    };
    (start + rounded / span) as u8
}

fn color_distance_sq(a: Rgba, b: Rgba) -> u32 {
    let channel = |left: u8, right: u8| {
        let delta = i32::from(left) - i32::from(right);
        (delta * delta) as u32
    };
    channel(a.r, b.r) + channel(a.g, b.g) + channel(a.b, b.b) + channel(a.a, b.a)
}

pub(crate) fn unpack(bytes: &[u8]) -> Option<Vec<Rgba>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| Rgba::new(c[0], c[1], c[2], c[3]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Rgba::parse_hex("#E8A838").unwrap();
        assert_eq!(c, Rgba::new(0xE8, 0xA8, 0x38, 255));
        assert_eq!(c.to_hex(), "#E8A838");
        let t = Rgba::parse_hex("11223380").unwrap();
        assert_eq!(t.a, 0x80);
        assert_eq!(t.to_hex(), "#11223380");
        assert!(Rgba::parse_hex("xyz").is_none());
        assert!(Rgba::parse_hex("#12345").is_none());
    }

    #[test]
    fn source_over_known_values() {
        let black = Rgba::new(0, 0, 0, 255);
        let white = Rgba::new(255, 255, 255, 255);
        assert_eq!(source_over(black, white, 255), white);
        assert_eq!(source_over(black, white, 0), black);
        assert_eq!(source_over(black, Rgba::CLEAR, 255), black);
        // 50% layer opacity of opaque white over black.
        assert_eq!(
            source_over(black, white, 128),
            Rgba::new(128, 128, 128, 255)
        );
    }

    #[test]
    fn color_ramp_keeps_the_ends_and_steps_evenly() {
        let start = Rgba::new(0, 0, 0, 0);
        let end = Rgba::new(10, 20, 30, 40);
        assert!(color_ramp(start, end, 0).is_none());
        assert!(color_ramp(start, end, 1).is_none());
        assert!(color_ramp(start, end, crate::limits::MAX_PALETTE + 1).is_none());
        assert_eq!(color_ramp(start, end, 2).unwrap(), vec![start, end]);
        assert_eq!(
            color_ramp(start, end, 3).unwrap(),
            vec![start, Rgba::new(5, 10, 15, 20), end]
        );
        let reds = color_ramp(Rgba::new(0, 0, 0, 255), Rgba::new(255, 0, 0, 255), 4).unwrap();
        assert_eq!(
            reds,
            vec![
                Rgba::new(0, 0, 0, 255),
                Rgba::new(85, 0, 0, 255),
                Rgba::new(170, 0, 0, 255),
                Rgba::new(255, 0, 0, 255),
            ]
        );
        let down = color_ramp(Rgba::new(10, 0, 0, 255), Rgba::new(0, 0, 0, 255), 3).unwrap();
        assert_eq!(down[1], Rgba::new(5, 0, 0, 255));
        assert_eq!(*down.first().unwrap(), Rgba::new(10, 0, 0, 255));
        assert_eq!(*down.last().unwrap(), Rgba::new(0, 0, 0, 255));
    }

    #[test]
    fn nearest_palette_color_breaks_ties_toward_the_earlier_swatch() {
        let red = Rgba::new(255, 0, 0, 255);
        let blue = Rgba::new(0, 0, 255, 255);
        let palette = [red, blue];
        assert_eq!(nearest_palette_color(red, &palette), red);
        assert_eq!(
            nearest_palette_color(Rgba::new(0, 200, 0, 255), &[red, Rgba::new(0, 255, 0, 255)]),
            Rgba::new(0, 255, 0, 255)
        );
        // (0, 255, 0) is the same distance from red and from blue. The earlier swatch wins.
        assert_eq!(
            nearest_palette_color(Rgba::new(0, 255, 0, 255), &palette),
            red
        );
        let clear = Rgba::CLEAR;
        let opaque = Rgba::new(0, 0, 0, 255);
        assert_eq!(
            nearest_palette_color(Rgba::new(0, 0, 0, 10), &[opaque, clear]),
            clear
        );
        assert_eq!(nearest_palette_color(red, &[]), red);
    }
}
