//! Headless check of the pixels the editor draws for the built-in example.
//!
//! File → Open example loads [`Editor::demo`]. The canvas texture and PNG
//! export both build that picture with [`Document::composite`]. This test
//! hashes those frames and compares them to `tests/golden/`. `cargo test`
//! runs it on Ubuntu and Windows. It stays off the window, file dialogs,
//! and any command server.
//!
//! The digest is SHA-256 (FIPS 180-4) of row-major RGBA8 bytes, `r g b a`.
//! `std::hash` is not used, because that hasher changes across Rust releases.
//!
//! Refresh the fixtures after an intentional change to the example or the
//! composite. Review the PNG diff, run the test again with the variable
//! unset, and commit `tests/golden/`:
//!
//! ```text
//! OXYPIXEL_UPDATE_DEMO_GOLDEN=1 cargo test -p oxypixel-core --test demo_composite -- --nocapture
//! ```
//!
//! CI leaves the variable unset. When `CI` or `GITHUB_ACTIONS` is set, the
//! test refuses to rewrite the fixtures.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use oxypixel_core::{decode_first_rgba, export_png_bytes, Document, Editor, Rgba};

const GOLDEN_HEADER: &str = "\
# SHA-256 of the built-in example after Document::composite.
# One line per frame: index width height lowercase-sha256
# The digest covers row-major RGBA8 bytes (r, g, b, a).
# demo-frame-N.png is that same frame from export_png_bytes.
# Update after an intentional change (leave this unset in CI):
#   OXYPIXEL_UPDATE_DEMO_GOLDEN=1 cargo test -p oxypixel-core --test demo_composite -- --nocapture
";

const UPDATE_HINT: &str = "OXYPIXEL_UPDATE_DEMO_GOLDEN=1 cargo test -p oxypixel-core --test demo_composite -- --nocapture";

#[test]
fn sha256_matches_fips_vectors() {
    // FIPS 180-4 / NIST one-block, empty, and two-block messages.
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn demo_composite_matches_golden() {
    let editor = Editor::demo().expect("built-in example");
    let doc = editor.document();
    let frames = composited(doc);
    let dir = golden_dir();

    if update_requested() {
        write_golden(&dir, &frames);
        eprintln!("updated fixtures in {}", dir.display());
    }

    let stored = read_golden(&dir, frames.len());
    for (index, frame) in frames.iter().enumerate() {
        let record = &stored[index];
        assert_eq!(
            record.index,
            index,
            "golden line {index} is out of order in {}",
            dir.display()
        );
        assert_eq!(
            (record.width, record.height),
            (frame.width, frame.height),
            "frame {index} size changed. Refresh with {UPDATE_HINT}"
        );
        let digest = hex(&sha256(&frame.rgba));
        assert_eq!(
            digest, record.digest,
            "frame {index} composite changed ({}x{}). Refresh with {UPDATE_HINT}",
            frame.width, frame.height
        );

        let png_path = dir.join(format!("demo-frame-{index}.png"));
        let png_bytes = fs::read(&png_path).unwrap_or_else(|err| {
            panic!(
                "missing golden {}: {err}. Refresh with {UPDATE_HINT}",
                png_path.display()
            )
        });
        let (width, height, pixels) = decode_png(&png_bytes, &png_path.display().to_string());
        assert_eq!(
            (width, height),
            (frame.width, frame.height),
            "golden {} has a different size. Refresh with {UPDATE_HINT}",
            png_path.display()
        );
        if pixels != frame.pixels {
            panic!(
                "golden {} pixels differ from the composite ({}). Refresh with {UPDATE_HINT}",
                png_path.display(),
                pixel_diffs(frame.width, &frame.pixels, &pixels)
            );
        }
    }
}

struct FramePixels {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
    rgba: Vec<u8>,
    png: Vec<u8>,
}

struct GoldenRecord {
    index: usize,
    width: u32,
    height: u32,
    digest: String,
}

fn composited(doc: &Document) -> Vec<FramePixels> {
    (0..doc.frame_count())
        .map(|index| {
            let pixels = doc
                .composite(index)
                .unwrap_or_else(|err| panic!("composite frame {index}: {err}"));
            let png = export_png_bytes(doc, index)
                .unwrap_or_else(|err| panic!("export frame {index}: {err}"));
            let (width, height, decoded) = decode_png(&png, &format!("export of frame {index}"));
            if width != doc.width() || height != doc.height() || decoded != pixels {
                panic!(
                    "export_png_bytes does not round-trip Document::composite for frame {index} ({width}x{height}, {})",
                    pixel_diffs(doc.width(), &pixels, &decoded)
                );
            }
            let rgba = rgba_bytes(&pixels);
            FramePixels {
                width: doc.width(),
                height: doc.height(),
                pixels,
                rgba,
                png,
            }
        })
        .collect()
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn update_requested() -> bool {
    let requested =
        env::var_os("OXYPIXEL_UPDATE_DEMO_GOLDEN").as_deref() == Some(std::ffi::OsStr::new("1"));
    if !requested {
        return false;
    }
    if env::var_os("CI").is_some() || env::var_os("GITHUB_ACTIONS").is_some() {
        panic!(
            "OXYPIXEL_UPDATE_DEMO_GOLDEN is set in CI. The golden files must already be committed."
        );
    }
    true
}

fn write_golden(dir: &Path, frames: &[FramePixels]) {
    fs::create_dir_all(dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let mut text = String::from(GOLDEN_HEADER);
    for (index, frame) in frames.iter().enumerate() {
        let digest = hex(&sha256(&frame.rgba));
        text.push_str(&format!(
            "{index} {} {} {digest}\n",
            frame.width, frame.height
        ));
        let path = dir.join(format!("demo-frame-{index}.png"));
        fs::write(&path, &frame.png)
            .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    }
    let manifest = dir.join("demo-composite.txt");
    fs::write(&manifest, text).unwrap_or_else(|err| panic!("write {}: {err}", manifest.display()));
}

fn read_golden(dir: &Path, frame_count: usize) -> Vec<GoldenRecord> {
    let manifest = dir.join("demo-composite.txt");
    let text = fs::read_to_string(&manifest).unwrap_or_else(|err| {
        panic!(
            "missing {}: {err}. Refresh with {UPDATE_HINT}",
            manifest.display()
        )
    });
    let mut records = Vec::new();
    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let index = field(&mut fields, line_no, "frame");
        let width = field(&mut fields, line_no, "width");
        let height = field(&mut fields, line_no, "height");
        let digest = fields.next().unwrap_or("").trim().to_ascii_lowercase();
        if fields.next().is_some()
            || digest.len() != 64
            || !digest.chars().all(|c| c.is_ascii_hexdigit())
        {
            panic!(
                "{}:{} is not 'index width height sha256'. Refresh with {UPDATE_HINT}",
                manifest.display(),
                line_no + 1
            );
        }
        records.push(GoldenRecord {
            index: parse_usize(index, &manifest, line_no),
            width: parse_u32(width, &manifest, line_no),
            height: parse_u32(height, &manifest, line_no),
            digest,
        });
    }
    assert_eq!(
        records.len(),
        frame_count,
        "{} lists {} frames, the example has {frame_count}. Refresh with {UPDATE_HINT}",
        manifest.display(),
        records.len()
    );
    records
}

fn field<'a>(fields: &mut impl Iterator<Item = &'a str>, line_no: usize, name: &str) -> &'a str {
    fields
        .next()
        .unwrap_or_else(|| panic!("line {}: missing {name}", line_no + 1))
}

fn parse_u32(text: &str, path: &Path, line_no: usize) -> u32 {
    text.parse().unwrap_or_else(|_| {
        panic!(
            "{}:{} has a bad number '{text}'",
            path.display(),
            line_no + 1
        )
    })
}

fn parse_usize(text: &str, path: &Path, line_no: usize) -> usize {
    text.parse().unwrap_or_else(|_| {
        panic!(
            "{}:{} has a bad number '{text}'",
            path.display(),
            line_no + 1
        )
    })
}

fn decode_png(bytes: &[u8], label: &str) -> (u32, u32, Vec<Rgba>) {
    decode_first_rgba(bytes).unwrap_or_else(|err| panic!("{label} is not a readable PNG: {err}"))
}

fn rgba_bytes(pixels: &[Rgba]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 4);
    for px in pixels {
        out.extend_from_slice(&px.to_bytes());
    }
    out
}

fn pixel_diffs(width: u32, got: &[Rgba], expected: &[Rgba]) -> String {
    if width == 0 || got.len() != expected.len() {
        return format!("pixel count {} != {}", got.len(), expected.len());
    }
    let mut parts = Vec::new();
    for (i, (left, right)) in got.iter().zip(expected).enumerate() {
        if left != right {
            let x = (i as u32) % width;
            let y = (i as u32) / width;
            parts.push(format!("({x},{y}) composite {left:?} golden {right:?}"));
            if parts.len() == 8 {
                parts.push("…".to_string());
                break;
            }
        }
    }
    if parts.is_empty() {
        "no channel differs".to_string()
    } else {
        parts.join("; ")
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let padded = pad(data);
    for chunk in padded.chunks_exact(64) {
        compress(&mut state, chunk);
    }
    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn pad(data: &[u8]) -> Vec<u8> {
    let bit_len = u64::try_from(data.len())
        .expect("sha256 input length fits in u64")
        .checked_mul(8)
        .expect("sha256 bit length fits in u64");
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    msg
}

fn compress(state: &mut [u32; 8], chunk: &[u8]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        let mut be = [0u8; 4];
        be.copy_from_slice(&chunk[i * 4..i * 4 + 4]);
        w[i] = u32::from_be_bytes(be);
    }
    for i in 16..64 {
        let s0 = sigma0(w[i - 15]);
        let s1 = sigma1(w[i - 2]);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];
    for i in 0..64 {
        let t1 = h
            .wrapping_add(big_sigma1(e))
            .wrapping_add(choice(e, f, g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let t2 = big_sigma0(a).wrapping_add(majority(a, b, c));
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

fn choice(e: u32, f: u32, g: u32) -> u32 {
    (e & f) ^ (!e & g)
}

fn majority(a: u32, b: u32, c: u32) -> u32 {
    (a & b) ^ (a & c) ^ (b & c)
}

fn big_sigma0(x: u32) -> u32 {
    x.rotate_right(2) ^ x.rotate_right(13) ^ x.rotate_right(22)
}

fn big_sigma1(x: u32) -> u32 {
    x.rotate_right(6) ^ x.rotate_right(11) ^ x.rotate_right(25)
}

fn sigma0(x: u32) -> u32 {
    x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
}

fn sigma1(x: u32) -> u32 {
    x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
}

#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
    0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
    0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
    0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
    0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
    0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
    0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
];
