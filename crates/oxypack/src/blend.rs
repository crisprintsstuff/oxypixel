//! Aseprite's layer blend modes.
//!
//! The standard modes follow the W3C "Compositing and Blending Level 1"
//! formulas; Addition, Subtract, and Divide are the plain clamped
//! operations their names describe. Normal is the core's `source_over`, so a
//! Normal layer composites exactly as before. Other modes are computed in
//! floating point and rounded, so they can differ from Aseprite by one step
//! in a channel.

use oxypixel_core::{source_over, Rgba};

/// The blend mode codes in an Aseprite layer chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Addition,
    Subtract,
    Divide,
}

impl BlendMode {
    /// `None` for a code this reader does not know.
    pub fn from_code(code: u16) -> Option<Self> {
        use BlendMode::*;
        Some(match code {
            0 => Normal,
            1 => Multiply,
            2 => Screen,
            3 => Overlay,
            4 => Darken,
            5 => Lighten,
            6 => ColorDodge,
            7 => ColorBurn,
            8 => HardLight,
            9 => SoftLight,
            10 => Difference,
            11 => Exclusion,
            12 => Hue,
            13 => Saturation,
            14 => Color,
            15 => Luminosity,
            16 => Addition,
            17 => Subtract,
            18 => Divide,
            _ => return None,
        })
    }
}

type Rgb = [f32; 3];

fn separable(mode: BlendMode, b: f32, s: f32) -> f32 {
    use BlendMode::*;
    match mode {
        Multiply => b * s,
        Screen => b + s - b * s,
        Overlay => separable(HardLight, s, b),
        Darken => b.min(s),
        Lighten => b.max(s),
        ColorDodge => {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }
        ColorBurn => {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }
        HardLight => {
            if s <= 0.5 {
                b * 2.0 * s
            } else {
                separable(Screen, b, 2.0 * s - 1.0)
            }
        }
        SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 {
                    ((16.0 * b - 12.0) * b + 4.0) * b
                } else {
                    b.sqrt()
                };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        Difference => (b - s).abs(),
        Exclusion => b + s - 2.0 * b * s,
        Addition => (b + s).min(1.0),
        Subtract => (b - s).max(0.0),
        Divide => {
            if s <= 0.0 {
                if b <= 0.0 {
                    0.0
                } else {
                    1.0
                }
            } else {
                (b / s).min(1.0)
            }
        }
        Normal | Hue | Saturation | Color | Luminosity => s,
    }
}

fn lum(c: Rgb) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: Rgb) -> Rgb {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        out = out.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        out = out.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    out
}

fn set_lum(c: Rgb, l: f32) -> Rgb {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: Rgb) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: Rgb, s: f32) -> Rgb {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    if max <= min {
        return [0.0; 3];
    }
    c.map(|v| (v - min) * s / (max - min))
}

fn mix(mode: BlendMode, b: Rgb, s: Rgb) -> Rgb {
    use BlendMode::*;
    match mode {
        Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        Color => set_lum(s, lum(b)),
        Luminosity => set_lum(b, lum(s)),
        _ => [0, 1, 2].map(|i| separable(mode, b[i], s[i])),
    }
}

/// Draw `src` over `dst` with `mode`, scaled by `opacity`.
pub fn blend(dst: Rgba, src: Rgba, opacity: u8, mode: BlendMode) -> Rgba {
    if mode == BlendMode::Normal {
        return source_over(dst, src, opacity);
    }
    let a_s = f32::from(src.a) / 255.0 * f32::from(opacity) / 255.0;
    if a_s <= 0.0 {
        return dst;
    }
    let a_b = f32::from(dst.a) / 255.0;
    let unit = |v: u8| f32::from(v) / 255.0;
    let b = [unit(dst.r), unit(dst.g), unit(dst.b)];
    let s = [unit(src.r), unit(src.g), unit(src.b)];
    let mixed = mix(mode, b, s);
    let a_o = a_s + a_b * (1.0 - a_s);
    let channel = |i: usize| {
        let premultiplied = a_s * (1.0 - a_b) * s[i]
            + a_s * a_b * mixed[i].clamp(0.0, 1.0)
            + (1.0 - a_s) * a_b * b[i];
        ((premultiplied / a_o).clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Rgba::new(
        channel(0),
        channel(1),
        channel(2),
        (a_o * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAY: Rgba = Rgba::new(128, 128, 128, 255);

    fn over_gray(mode: BlendMode, src: Rgba) -> Rgba {
        blend(GRAY, src, 255, mode)
    }

    #[test]
    fn separable_modes_on_an_opaque_backdrop() {
        let red = Rgba::new(255, 0, 0, 255);
        assert_eq!(
            over_gray(BlendMode::Multiply, red),
            Rgba::new(128, 0, 0, 255)
        );
        assert_eq!(
            over_gray(BlendMode::Screen, red),
            Rgba::new(255, 128, 128, 255)
        );
        assert_eq!(over_gray(BlendMode::Darken, red), Rgba::new(128, 0, 0, 255));
        assert_eq!(
            over_gray(BlendMode::Lighten, red),
            Rgba::new(255, 128, 128, 255)
        );
        assert_eq!(
            over_gray(BlendMode::Difference, red),
            Rgba::new(127, 128, 128, 255)
        );
        assert_eq!(
            over_gray(BlendMode::Addition, red),
            Rgba::new(255, 128, 128, 255)
        );
        assert_eq!(
            over_gray(BlendMode::Subtract, red),
            Rgba::new(0, 128, 128, 255)
        );
        // Overlay and hard light swap which layer decides the curve.
        let dark = Rgba::new(64, 64, 64, 255);
        assert_eq!(blend(dark, GRAY, 255, BlendMode::Overlay).r, 64);
        assert_eq!(blend(GRAY, dark, 255, BlendMode::HardLight).r, 64);
        // Divide by black is white where the backdrop has light.
        assert_eq!(over_gray(BlendMode::Divide, Rgba::new(0, 0, 0, 255)).r, 255);
    }

    #[test]
    fn normal_is_source_over_and_clear_or_zero_opacity_changes_nothing() {
        let src = Rgba::new(10, 200, 30, 90);
        assert_eq!(
            blend(GRAY, src, 140, BlendMode::Normal),
            source_over(GRAY, src, 140)
        );
        assert_eq!(blend(GRAY, Rgba::CLEAR, 255, BlendMode::Multiply), GRAY);
        assert_eq!(blend(GRAY, src, 0, BlendMode::Screen), GRAY);
    }

    #[test]
    fn a_mode_over_clear_pixels_is_the_source_and_half_opacity_is_halfway() {
        let blue = Rgba::new(0, 0, 255, 255);
        assert_eq!(blend(Rgba::CLEAR, blue, 255, BlendMode::Multiply), blue);
        let half = blend(
            GRAY,
            Rgba::new(255, 255, 255, 255),
            128,
            BlendMode::Multiply,
        );
        assert_eq!(half, GRAY);
        let half_screen = blend(GRAY, Rgba::new(255, 255, 255, 255), 128, BlendMode::Screen);
        assert_eq!(half_screen.r, 192);
    }

    #[test]
    fn hue_saturation_color_and_luminosity_keep_the_other_parts() {
        let red = Rgba::new(255, 0, 0, 255);
        let gray = Rgba::new(100, 100, 100, 255);
        // A gray source has no saturation, so Saturation turns red gray.
        let out = blend(red, gray, 255, BlendMode::Saturation);
        assert!(out.r == out.g && out.g == out.b, "{out:?}");
        // Luminosity of black over red is black.
        assert_eq!(
            blend(red, Rgba::new(0, 0, 0, 255), 255, BlendMode::Luminosity),
            Rgba::new(0, 0, 0, 255)
        );
        // Color takes the source hue and keeps the backdrop's lightness.
        let tinted = blend(gray, red, 255, BlendMode::Color);
        assert!(tinted.r > tinted.g && tinted.g == tinted.b, "{tinted:?}");
        let hue = blend(
            Rgba::new(200, 100, 100, 255),
            Rgba::new(0, 0, 255, 255),
            255,
            BlendMode::Hue,
        );
        assert!(hue.b > hue.r && hue.r == hue.g, "{hue:?}");
        assert_eq!(BlendMode::from_code(18), Some(BlendMode::Divide));
        assert_eq!(BlendMode::from_code(19), None);
    }
}
