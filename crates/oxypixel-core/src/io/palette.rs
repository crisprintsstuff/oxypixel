//! Palette text files.
//!
//! Three formats, chosen from the header or the file extension:
//!
//! - GIMP `.gpl`. The first non-empty line is `GIMP Palette`. `Name:` and
//!   `Columns:` are ignored. `#` starts a comment. A color is `R G B` or
//!   `R G B A` with channels from 0 to 255. A fourth number is alpha only when
//!   it is the last token; anything after that is a color name and is ignored.
//! - Hex `.hex` (and `.txt`). One `RRGGBB` or `RRGGBBAA` per line. A leading
//!   `#` is allowed on a color. Blank lines, `;` comments, `//` comments, and
//!   `#` comments (a hash with nothing or whitespace after it) are skipped.
//! - JASC `.pal`. The header is `JASC-PAL`, then `0100`, then a color count,
//!   then that many `R G B` lines. Alpha is not stored. `#` and `;` comments
//!   and blank lines are skipped and do not count toward the color count.
//!
//! More than [`MAX_PALETTE`](crate::limits::MAX_PALETTE) colors is an error.
//! The count in a JASC file is checked before those lines are read.

use std::path::Path;

use crate::color::Rgba;
use crate::error::{Error, Result};
use crate::limits::{check_file_len, MAX_PALETTE};

/// Which palette text format to write, or which one a path names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteFormat {
    /// GIMP palette, extension `gpl`.
    Gpl,
    /// One hex color per line, extension `hex` or `txt`.
    Hex,
    /// JASC palette, extension `pal`.
    Pal,
}

impl PaletteFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Gpl => "gpl",
            Self::Hex => "hex",
            Self::Pal => "pal",
        }
    }

    /// `gpl`, `hex`, `txt` (hex), or `pal`. The dot and letter case are ignored.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "gpl" => Some(Self::Gpl),
            "hex" | "txt" => Some(Self::Hex),
            "pal" => Some(Self::Pal),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }
}

/// Read a palette file. The extension is a hint. A `GIMP Palette` or `JASC-PAL`
/// header wins over the extension.
pub fn import_palette(path: &Path) -> Result<Vec<Rgba>> {
    let bytes = super::oxypix::read_limited(path)?;
    let extension = path.extension().and_then(|ext| ext.to_str());
    import_palette_bytes(&bytes, extension)
}

/// Parse palette bytes. `extension` may be `gpl`, `hex`, `txt`, or `pal`.
pub fn import_palette_bytes(bytes: &[u8], extension: Option<&str>) -> Result<Vec<Rgba>> {
    check_file_len(bytes.len() as u64)?;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::BadPalette("the palette file is not valid text".to_string()))?;
    import_palette_text(text, extension)
}

/// Write the palette. The output extension selects the format.
pub fn export_palette(colors: &[Rgba], path: &Path) -> Result<()> {
    let format = PaletteFormat::from_path(path).ok_or_else(|| {
        Error::BadPalette(
            "unsupported palette file (use GIMP .gpl, hex .hex, or JASC .pal)".to_string(),
        )
    })?;
    let text = export_palette_string(colors, format)?;
    std::fs::write(path, text.as_bytes()).map_err(Error::io)
}

/// Serialize a palette. An empty list, or one longer than the palette cap, is an error.
pub fn export_palette_string(colors: &[Rgba], format: PaletteFormat) -> Result<String> {
    if colors.is_empty() {
        return Err(Error::EmptyPalette);
    }
    if colors.len() > MAX_PALETTE {
        return Err(Error::Limit {
            what: "palette colors",
            value: colors.len() as u64,
            max: MAX_PALETTE as u64,
        });
    }
    let mut out = String::new();
    match format {
        PaletteFormat::Gpl => {
            out.push_str("GIMP Palette\nName: OxyPixel\nColumns: 0\n#\n");
            for color in colors {
                if color.a == 255 {
                    out.push_str(&format!("{} {} {}\n", color.r, color.g, color.b));
                } else {
                    out.push_str(&format!(
                        "{} {} {} {}\n",
                        color.r, color.g, color.b, color.a
                    ));
                }
            }
        }
        PaletteFormat::Hex => {
            for color in colors {
                if color.a == 255 {
                    out.push_str(&format!("{:02X}{:02X}{:02X}\n", color.r, color.g, color.b));
                } else {
                    out.push_str(&format!(
                        "{:02X}{:02X}{:02X}{:02X}\n",
                        color.r, color.g, color.b, color.a
                    ));
                }
            }
        }
        PaletteFormat::Pal => {
            out.push_str(&format!("JASC-PAL\n0100\n{}\n", colors.len()));
            for color in colors {
                out.push_str(&format!("{} {} {}\n", color.r, color.g, color.b));
            }
        }
    }
    Ok(out)
}

fn import_palette_text(text: &str, extension: Option<&str>) -> Result<Vec<Rgba>> {
    check_file_len(text.len() as u64)?;
    let text = text.trim_start_matches('\u{feff}');
    match choose_format(text, extension)? {
        PaletteFormat::Gpl => parse_gpl(text),
        PaletteFormat::Hex => parse_hex(text),
        PaletteFormat::Pal => parse_pal(text),
    }
}

fn choose_format(text: &str, extension: Option<&str>) -> Result<PaletteFormat> {
    match first_nonempty(text) {
        Some("GIMP Palette") => return Ok(PaletteFormat::Gpl),
        Some("JASC-PAL") => return Ok(PaletteFormat::Pal),
        _ => {}
    }
    extension
        .and_then(PaletteFormat::from_extension)
        .ok_or_else(|| {
            Error::BadPalette(
                "unsupported palette file (use GIMP .gpl, hex .hex, or JASC .pal)".to_string(),
            )
        })
}

fn first_nonempty(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

fn numbered(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
}

fn parse_gpl(text: &str) -> Result<Vec<Rgba>> {
    let mut lines = numbered(text);
    let (line_no, first) = loop {
        match lines.next() {
            Some((_, "")) => continue,
            Some(item) => break item,
            None => return Err(bad("GIMP", 1, "expected the header GIMP Palette")),
        }
    };
    if first != "GIMP Palette" {
        return Err(bad("GIMP", line_no, "expected the header GIMP Palette"));
    }
    let mut colors = Vec::new();
    for (line_no, line) in lines {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("Name:") || line.starts_with("Columns:") {
            continue;
        }
        let color = parse_gpl_color(line).map_err(|detail| bad("GIMP", line_no, detail))?;
        push_color(&mut colors, color)?;
    }
    finish(colors)
}

fn parse_gpl_color(line: &str) -> std::result::Result<Rgba, &'static str> {
    let mut parts = line.split_whitespace();
    let r = channel(
        parts
            .next()
            .ok_or("expected red, green, and blue from 0 to 255")?,
    )?;
    let g = channel(
        parts
            .next()
            .ok_or("expected red, green, and blue from 0 to 255")?,
    )?;
    let b = channel(
        parts
            .next()
            .ok_or("expected red, green, and blue from 0 to 255")?,
    )?;
    let alpha = match parts.next() {
        None => 255,
        Some(token) => {
            if parts.next().is_some() || !token.bytes().all(|byte| byte.is_ascii_digit()) {
                255
            } else {
                channel(token)?
            }
        }
    };
    Ok(Rgba::new(r, g, b, alpha))
}

fn parse_hex(text: &str) -> Result<Vec<Rgba>> {
    let mut colors = Vec::new();
    for (line_no, line) in numbered(text) {
        match classify_hex(line) {
            HexLine::Skip => {}
            HexLine::Color(color) => push_color(&mut colors, color)?,
            HexLine::Bad => return Err(bad("hex", line_no, "expected RRGGBB or RRGGBBAA")),
        }
    }
    finish(colors)
}

enum HexLine {
    Skip,
    Color(Rgba),
    Bad,
}

fn classify_hex(line: &str) -> HexLine {
    if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
        return HexLine::Skip;
    }
    if let Some(rest) = line.strip_prefix('#') {
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            return HexLine::Skip;
        }
        return hex_color(rest.trim());
    }
    hex_color(line)
}

fn hex_color(text: &str) -> HexLine {
    if !is_plain_hex(text) {
        return HexLine::Bad;
    }
    match Rgba::parse_hex(text) {
        Some(color) => HexLine::Color(color),
        None => HexLine::Bad,
    }
}

fn is_plain_hex(text: &str) -> bool {
    let len = text.len();
    (len == 6 || len == 8) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_pal(text: &str) -> Result<Vec<Rgba>> {
    let mut lines = numbered(text).filter(|(_, line)| !line.is_empty() && !is_pal_comment(line));
    let Some((magic_line, magic)) = lines.next() else {
        return Err(bad("JASC", 1, "expected the header JASC-PAL"));
    };
    if magic != "JASC-PAL" {
        return Err(bad("JASC", magic_line, "expected the header JASC-PAL"));
    }
    let Some((version_line, version)) = lines.next() else {
        return Err(bad("JASC", magic_line, "expected version 0100"));
    };
    if version != "0100" {
        return Err(bad("JASC", version_line, "expected version 0100"));
    }
    let Some((count_line, count_text)) = lines.next() else {
        return Err(bad("JASC", version_line, "expected a color count"));
    };
    if count_text.is_empty() || !count_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(bad("JASC", count_line, "expected a color count"));
    }
    let count: u64 = match count_text.parse() {
        Ok(count) => count,
        Err(_) => return Err(bad("JASC", count_line, "expected a color count")),
    };
    if count == 0 {
        return Err(Error::EmptyPalette);
    }
    if count > MAX_PALETTE as u64 {
        return Err(Error::Limit {
            what: "palette colors",
            value: count,
            max: MAX_PALETTE as u64,
        });
    }
    let count = count as usize;
    let mut colors = Vec::with_capacity(count);
    let mut last_line = count_line;
    for _ in 0..count {
        let Some((line_no, line)) = lines.next() else {
            return Err(bad(
                "JASC",
                last_line,
                "the file ended before every color was listed",
            ));
        };
        last_line = line_no;
        let color = parse_jasc_color(line).map_err(|detail| bad("JASC", line_no, detail))?;
        colors.push(color);
    }
    if let Some((line_no, _)) = lines.next() {
        return Err(bad(
            "JASC",
            line_no,
            "the file has more colors than its count",
        ));
    }
    Ok(colors)
}

fn is_pal_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with(';')
}

fn parse_jasc_color(line: &str) -> std::result::Result<Rgba, &'static str> {
    let mut parts = line.split_whitespace();
    let r = channel(
        parts
            .next()
            .ok_or("expected red, green, and blue from 0 to 255")?,
    )?;
    let g = channel(
        parts
            .next()
            .ok_or("expected red, green, and blue from 0 to 255")?,
    )?;
    let b = channel(
        parts
            .next()
            .ok_or("expected red, green, and blue from 0 to 255")?,
    )?;
    if parts.next().is_some() {
        return Err("expected red, green, and blue from 0 to 255");
    }
    Ok(Rgba::new(r, g, b, 255))
}

fn channel(token: &str) -> std::result::Result<u8, &'static str> {
    channel_opt(token).ok_or("expected red, green, and blue from 0 to 255")
}

fn channel_opt(token: &str) -> Option<u8> {
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = token.parse::<u16>().ok()?;
    u8::try_from(value).ok()
}

fn push_color(colors: &mut Vec<Rgba>, color: Rgba) -> Result<()> {
    if colors.len() >= MAX_PALETTE {
        return Err(Error::Limit {
            what: "palette colors",
            value: (colors.len() as u64).saturating_add(1),
            max: MAX_PALETTE as u64,
        });
    }
    colors.push(color);
    Ok(())
}

fn finish(colors: Vec<Rgba>) -> Result<Vec<Rgba>> {
    if colors.is_empty() {
        Err(Error::EmptyPalette)
    } else {
        Ok(colors)
    }
}

fn bad(format: &str, line: usize, detail: &str) -> Error {
    Error::BadPalette(format!("{format} palette, line {line}: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Rgba> {
        vec![
            Rgba::new(255, 0, 0, 255),
            Rgba::new(0, 128, 255, 255),
            Rgba::new(10, 20, 30, 128),
            Rgba::new(1, 2, 3, 0),
        ]
    }

    fn assert_line(err: Error, line: usize) {
        let Error::BadPalette(message) = err else {
            panic!("expected a palette error");
        };
        assert!(
            message.contains(&format!("line {line}")),
            "message was {message}"
        );
    }

    fn assert_limit(err: Error, value: u64) {
        match err {
            Error::Limit {
                what,
                value: got,
                max,
            } => {
                assert_eq!(what, "palette colors");
                assert_eq!(got, value);
                assert_eq!(max, MAX_PALETTE as u64);
            }
            other => panic!("expected a palette limit, got {other}"),
        }
    }

    #[test]
    fn gpl_and_hex_roundtrip_including_alpha() {
        let colors = sample();
        for format in [PaletteFormat::Gpl, PaletteFormat::Hex] {
            let text = export_palette_string(&colors, format).unwrap();
            let back = import_palette_bytes(text.as_bytes(), Some(format.extension())).unwrap();
            assert_eq!(back, colors);
        }
    }

    #[test]
    fn jasc_roundtrip_drops_alpha() {
        let text = export_palette_string(&sample(), PaletteFormat::Pal).unwrap();
        let back = import_palette_bytes(text.as_bytes(), Some("pal")).unwrap();
        assert_eq!(
            back,
            vec![
                Rgba::new(255, 0, 0, 255),
                Rgba::new(0, 128, 255, 255),
                Rgba::new(10, 20, 30, 255),
                Rgba::new(1, 2, 3, 255),
            ]
        );
    }

    #[test]
    fn export_text_matches_each_format() {
        let colors = [Rgba::new(255, 0, 0, 255), Rgba::new(1, 2, 3, 128)];
        assert_eq!(
            export_palette_string(&colors, PaletteFormat::Gpl).unwrap(),
            "GIMP Palette\nName: OxyPixel\nColumns: 0\n#\n255 0 0\n1 2 3 128\n"
        );
        assert_eq!(
            export_palette_string(&colors, PaletteFormat::Hex).unwrap(),
            "FF0000\n01020380\n"
        );
        assert_eq!(
            export_palette_string(&colors, PaletteFormat::Pal).unwrap(),
            "JASC-PAL\n0100\n2\n255 0 0\n1 2 3\n"
        );
    }

    #[test]
    fn gpl_skips_comments_blank_lines_and_names() {
        let text = "\u{feff}GIMP Palette\r\nName: Sample\r\nColumns: 16\r\n#\r\n# a comment\r\n\r\n  8   16   32\tink\r\n255 128 64 named color\r\n10 20 30 40\r\n";
        let colors = import_palette_bytes(text.as_bytes(), Some("gpl")).unwrap();
        assert_eq!(
            colors,
            vec![
                Rgba::new(8, 16, 32, 255),
                Rgba::new(255, 128, 64, 255),
                Rgba::new(10, 20, 30, 40),
            ]
        );
    }

    #[test]
    fn hex_skips_comments_and_blank_lines() {
        let text = "; Paint.NET style\n\n# a comment\n// also\n\nff0000\n#00FF00\n01020380\n";
        let colors = import_palette_bytes(text.as_bytes(), Some("hex")).unwrap();
        assert_eq!(
            colors,
            vec![
                Rgba::new(255, 0, 0, 255),
                Rgba::new(0, 255, 0, 255),
                Rgba::new(1, 2, 3, 128),
            ]
        );
        let as_txt = import_palette_bytes(text.as_bytes(), Some("txt")).unwrap();
        assert_eq!(as_txt, colors);
    }

    #[test]
    fn jasc_skips_comments_and_blank_lines() {
        let text = "JASC-PAL\n\n# comment\n0100\n\n; another\n2\n\n1 2 3\n\n4 5 6\n";
        let colors = import_palette_bytes(text.as_bytes(), Some("pal")).unwrap();
        assert_eq!(
            colors,
            vec![Rgba::new(1, 2, 3, 255), Rgba::new(4, 5, 6, 255)]
        );
    }

    #[test]
    fn header_wins_over_a_mismatched_extension() {
        let text = "GIMP Palette\n9 8 7\n";
        let colors = import_palette_bytes(text.as_bytes(), Some("hex")).unwrap();
        assert_eq!(colors, vec![Rgba::new(9, 8, 7, 255)]);
    }

    #[test]
    fn malformed_lines_name_the_line() {
        assert_line(
            import_palette_bytes(b"255 0 0\n", Some("gpl")).unwrap_err(),
            1,
        );
        assert_line(
            import_palette_bytes(b"GIMP Palette\n300 0 0\n", Some("gpl")).unwrap_err(),
            2,
        );
        assert_line(
            import_palette_bytes(b"GIMP Palette\n-1 0 0\n", Some("gpl")).unwrap_err(),
            2,
        );
        assert_line(
            import_palette_bytes(b"GIMP Palette\n1 2 3 300\n", Some("gpl")).unwrap_err(),
            2,
        );
        assert_line(
            import_palette_bytes(b"; comment\n\n12\n", Some("hex")).unwrap_err(),
            3,
        );
        assert_line(
            import_palette_bytes(b"FF0000\n#gg0000\n", Some("hex")).unwrap_err(),
            2,
        );
        assert_line(
            import_palette_bytes(b"jasc-pal\n0100\n1\n0 0 0\n", Some("pal")).unwrap_err(),
            1,
        );
        assert_line(
            import_palette_bytes(b"JASC-PAL\n0200\n1\n0 0 0\n", Some("pal")).unwrap_err(),
            2,
        );
        assert_line(
            import_palette_bytes(b"JASC-PAL\n0100\n1\n0 0\n", Some("pal")).unwrap_err(),
            4,
        );
        assert_line(
            import_palette_bytes(b"JASC-PAL\n0100\n1\n0 0 0\n1 1 1\n", Some("pal")).unwrap_err(),
            5,
        );
        assert!(matches!(
            import_palette_bytes(b"JASC-PAL\n0100\n1\n", Some("pal")).unwrap_err(),
            Error::BadPalette(_)
        ));
    }

    #[test]
    fn empty_files_need_a_color() {
        assert!(matches!(
            import_palette_bytes(b"\n\n", Some("hex")).unwrap_err(),
            Error::EmptyPalette
        ));
        assert!(matches!(
            import_palette_bytes(b"GIMP Palette\n# only a comment\n", Some("gpl")).unwrap_err(),
            Error::EmptyPalette
        ));
        assert!(matches!(
            import_palette_bytes(b"JASC-PAL\n0100\n0\n", Some("pal")).unwrap_err(),
            Error::EmptyPalette
        ));
        assert!(matches!(
            export_palette_string(&[], PaletteFormat::Hex).unwrap_err(),
            Error::EmptyPalette
        ));
    }

    #[test]
    fn invalid_text_and_unknown_extension_are_errors() {
        assert!(matches!(
            import_palette_bytes(&[0xff, 0xfe], Some("gpl")).unwrap_err(),
            Error::BadPalette(_)
        ));
        assert!(matches!(
            import_palette_bytes(b"FF0000\n", None).unwrap_err(),
            Error::BadPalette(_)
        ));
        let err = export_palette(&[Rgba::new(0, 0, 0, 255)], Path::new("palette.bmp")).unwrap_err();
        assert!(matches!(err, Error::BadPalette(_)));
        let err = import_palette(Path::new("this-palette-does-not-exist.gpl")).unwrap_err();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn palette_cap_is_an_error_not_a_truncated_list() {
        let mut hex = String::new();
        let mut gpl = String::from("GIMP Palette\n#\n");
        for i in 0..257 {
            let channel = (i % 256) as u8;
            hex.push_str(&format!("{channel:02X}0000\n"));
            gpl.push_str(&format!("{channel} 0 0\n"));
        }
        assert_limit(
            import_palette_bytes(hex.as_bytes(), Some("hex")).unwrap_err(),
            257,
        );
        assert_limit(
            import_palette_bytes(gpl.as_bytes(), Some("gpl")).unwrap_err(),
            257,
        );
        let claimed = b"JASC-PAL\n0100\n1000\n0 0 0\n";
        assert_limit(
            import_palette_bytes(claimed, Some("pal")).unwrap_err(),
            1000,
        );
        let mut exact = String::from("JASC-PAL\n0100\n256\n");
        for _ in 0..256 {
            exact.push_str("1 2 3\n");
        }
        assert_eq!(
            import_palette_bytes(exact.as_bytes(), Some("pal"))
                .unwrap()
                .len(),
            256
        );
        let mut too_many = vec![Rgba::new(0, 0, 0, 255); 257];
        assert_limit(
            export_palette_string(&too_many, PaletteFormat::Hex).unwrap_err(),
            257,
        );
        too_many.pop();
        assert_eq!(
            import_palette_bytes(
                export_palette_string(&too_many, PaletteFormat::Hex)
                    .unwrap()
                    .as_bytes(),
                Some("hex"),
            )
            .unwrap()
            .len(),
            256
        );
    }

    #[test]
    fn path_roundtrip_writes_and_reads_gpl() {
        let dir = std::env::temp_dir().join(format!(
            "oxypixel-palette-core-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ink.gpl");
        let colors = sample();
        export_palette(&colors, &path).unwrap();
        assert_eq!(import_palette(&path).unwrap(), colors);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
