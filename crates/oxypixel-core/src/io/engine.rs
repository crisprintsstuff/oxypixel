//! Files that game engines import beside a spritesheet, and one file per tag.
//!
//! Godot 4 reads a `SpriteFrames` resource (`.tres`) with one animation per
//! tag. Unity reads the sprite slices from the sheet's `.png.meta`. Both are
//! plain text written from the engines' public file formats.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::limits::DEFAULT_FRAME_MS;
use crate::tags::{FrameTag, LoopDirection};

use super::raster::{gif_bytes, sheet_grid, sheet_json, sheet_png, SheetGrid, SheetLayout};

/// What to write beside a spritesheet PNG.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SheetFiles {
    /// `<name>.json`, the JSON array sheet.
    pub json: bool,
    /// `<name>.tres`, a Godot 4 `SpriteFrames` resource.
    pub godot: bool,
    /// `<name>.png.meta`, Unity's import settings with one sprite per frame.
    pub unity: bool,
}

/// Write the spritesheet at `path` and the files `files` asks for. With
/// `per_tag`, write one sheet per tag instead, named `<name>_<tag>.png`.
/// Nothing is written unless every file could be made. Returns the paths
/// written, sheets first.
pub fn export_sheet_files(
    doc: &Document,
    layout: SheetLayout,
    path: &Path,
    files: SheetFiles,
    per_tag: bool,
) -> Result<Vec<PathBuf>> {
    let mut outputs = Vec::new();
    if per_tag {
        let tags = doc.tags();
        if tags.is_empty() {
            return Err(Error::BadTag("the document has no tags".into()));
        }
        for (tag, sheet) in tags.iter().zip(tag_paths(path, tags)) {
            let frames: Vec<usize> = (tag.from..=tag.to).collect();
            let local = FrameTag {
                from: 0,
                to: tag.to - tag.from,
                ..tag.clone()
            };
            sheet_outputs(doc, &frames, &[local], layout, &sheet, files, &mut outputs)?;
        }
    } else {
        let frames: Vec<usize> = (0..doc.frame_count()).collect();
        sheet_outputs(doc, &frames, doc.tags(), layout, path, files, &mut outputs)?;
    }
    write_all(&outputs)?;
    Ok(outputs.into_iter().map(|(path, _)| path).collect())
}

/// One GIF per tag, named `<name>_<tag>.gif`, playing the tag's frames in
/// its direction and as many times as it repeats. Returns the paths written.
pub fn export_gif_per_tag(doc: &Document, path: &Path) -> Result<Vec<PathBuf>> {
    let tags = doc.tags();
    if tags.is_empty() {
        return Err(Error::BadTag("the document has no tags".into()));
    }
    let mut outputs = Vec::new();
    for (tag, gif) in tags.iter().zip(tag_paths(path, tags)) {
        outputs.push((gif, gif_bytes(doc, &play_order(tag), tag.repeat)?));
    }
    write_all(&outputs)?;
    Ok(outputs.into_iter().map(|(path, _)| path).collect())
}

fn sheet_outputs(
    doc: &Document,
    frames: &[usize],
    tags: &[FrameTag],
    layout: SheetLayout,
    path: &Path,
    files: SheetFiles,
    outputs: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<()> {
    let image = file_name(path);
    let grid = sheet_grid(doc, frames.len(), layout)?;
    outputs.push((path.to_path_buf(), sheet_png(doc, frames, layout)?));
    if files.json {
        let json = sheet_json(doc, frames, tags, layout, &image)?;
        outputs.push((path.with_extension("json"), json.into_bytes()));
    }
    if files.godot {
        let tres = godot_sprite_frames(doc, frames, tags, &grid, &image);
        outputs.push((path.with_extension("tres"), tres.into_bytes()));
    }
    if files.unity {
        let meta_path = PathBuf::from(format!("{}.meta", path.display()));
        let guid = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|text| existing_guid(&text))
            .unwrap_or_else(|| new_guid(path));
        let stem = path
            .file_stem()
            .map(|stem| safe_name(&stem.to_string_lossy()))
            .unwrap_or_else(|| "sprite".into());
        outputs.push((meta_path, unity_meta(&grid, &stem, &guid).into_bytes()));
    }
    Ok(())
}

fn write_all(outputs: &[(PathBuf, Vec<u8>)]) -> Result<()> {
    for (path, bytes) in outputs {
        std::fs::write(path, bytes).map_err(Error::io)?;
    }
    Ok(())
}

/// The frames a tag plays in one pass, in order.
pub fn play_order(tag: &FrameTag) -> Vec<usize> {
    let forward: Vec<usize> = (tag.from..=tag.to).collect();
    let backward: Vec<usize> = forward.iter().rev().copied().collect();
    // A ping-pong pass turns at both ends without showing them twice.
    let bounce = |there: &[usize], back: &[usize]| {
        let mut out = there.to_vec();
        if back.len() > 2 {
            out.extend_from_slice(&back[1..back.len() - 1]);
        }
        out
    };
    match tag.direction {
        LoopDirection::Forward => forward,
        LoopDirection::Reverse => backward,
        LoopDirection::PingPong => bounce(&forward, &backward),
        LoopDirection::PingPongReverse => bounce(&backward, &forward),
    }
}

/// `<dir>/<stem>_<tag>.<ext>` for each tag. Characters a file name can't
/// hold become `_`, and a name already used gets `_2`, `_3`, and so on.
pub fn tag_paths(path: &Path, tags: &[FrameTag]) -> Vec<PathBuf> {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    let mut used: Vec<String> = Vec::new();
    tags.iter()
        .map(|tag| {
            let base = format!("{stem}_{}", safe_name(&tag.name));
            let mut name = base.clone();
            let mut n = 2;
            while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
                name = format!("{base}_{n}");
                n += 1;
            }
            used.push(name.clone());
            path.with_file_name(format!("{name}{ext}"))
        })
        .collect()
}

/// Letters, digits, `-`, and `_` stay; anything else becomes `_`.
fn safe_name(text: &str) -> String {
    let name: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "_".into()
    } else {
        name
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A Godot 4 `SpriteFrames` resource for a sheet of `frames`. Each tag is an
/// animation (`tags` count from 0 at the first cell); with no tags there is
/// one animation, `default`, of every frame. Frame durations become Godot's
/// relative durations at a speed of one step per shared millisecond count.
/// The texture path is the sheet's file name, so the `.tres` must stay
/// beside the PNG in the project.
pub fn godot_sprite_frames(
    doc: &Document,
    frames: &[usize],
    tags: &[FrameTag],
    grid: &SheetGrid,
    image: &str,
) -> String {
    let mut out = format!(
        "[gd_resource type=\"SpriteFrames\" load_steps={} format=3]\n\n[ext_resource type=\"Texture2D\" path={} id=\"1_sheet\"]\n\n",
        grid.cells.len() + 2,
        godot_string(image),
    );
    for (cell, (x, y)) in grid.cells.iter().enumerate() {
        out.push_str(&format!(
            "[sub_resource type=\"AtlasTexture\" id=\"AtlasTexture_{cell}\"]\natlas = ExtResource(\"1_sheet\")\nregion = Rect2({x}, {y}, {}, {})\n\n",
            grid.cell_width, grid.cell_height,
        ));
    }
    let whole;
    let animations: Vec<(String, Vec<usize>, bool)> = if tags.is_empty() {
        whole = FrameTag {
            name: "default".into(),
            from: 0,
            to: frames.len().saturating_sub(1),
            direction: LoopDirection::Forward,
            repeat: 0,
        };
        vec![(whole.name.clone(), play_order(&whole), true)]
    } else {
        let mut names: Vec<String> = Vec::new();
        tags.iter()
            .map(|tag| {
                // Godot needs each animation name once.
                let mut name = tag.name.clone();
                let mut n = 2;
                while names.contains(&name) {
                    name = format!("{} {n}", tag.name);
                    n += 1;
                }
                names.push(name.clone());
                (name, play_order(tag), tag.repeat == 0)
            })
            .collect()
    };
    out.push_str("[resource]\nanimations = [");
    for (i, (name, cells, looped)) in animations.iter().enumerate() {
        let ms: Vec<u32> = cells
            .iter()
            .map(|&cell| {
                frames
                    .get(cell)
                    .and_then(|&frame| doc.frame_duration(frame))
                    .unwrap_or(DEFAULT_FRAME_MS)
            })
            .collect();
        let step = ms.iter().copied().fold(0, gcd).max(1);
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str("{\n\"frames\": [");
        for (j, (&cell, &ms)) in cells.iter().zip(&ms).enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!(
                "{{\n\"duration\": {},\n\"texture\": SubResource(\"AtlasTexture_{cell}\")\n}}",
                godot_float(f64::from(ms / step)),
            ));
        }
        out.push_str(&format!(
            "],\n\"loop\": {looped},\n\"name\": &{},\n\"speed\": {}\n}}",
            godot_string(name),
            godot_float(1000.0 / f64::from(step)),
        ));
    }
    out.push_str("]\n");
    out
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// A float as Godot writes one: always with a decimal point.
fn godot_float(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains('e') {
        text
    } else {
        format!("{text}.0")
    }
}

fn godot_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Unity import settings for a sheet: a point-filtered, uncompressed sprite
/// texture in Multiple mode with one sprite per cell, named `<stem>_<i>`.
/// Unity counts sprite rectangles from the bottom left.
pub fn unity_meta(grid: &SheetGrid, stem: &str, guid: &str) -> String {
    let (w, h) = (grid.cell_width, grid.cell_height);
    let mut sprites = String::new();
    let mut table = String::new();
    for (i, (x, y)) in grid.cells.iter().enumerate() {
        let name = format!("{stem}_{i}");
        // Unity's own numbering for sprites in a texture.
        let id = 21_300_000 + 2 * i as i64;
        let unity_y = grid.height - y - h;
        sprites.push_str(&format!(
            "    - serializedVersion: 2
      name: {name}
      rect:
        serializedVersion: 2
        x: {x}
        y: {unity_y}
        width: {w}
        height: {h}
      alignment: 0
      pivot: {{x: 0.5, y: 0.5}}
      border: {{x: 0, y: 0, z: 0, w: 0}}
      outline: []
      physicsShape: []
      tessellationDetail: 0
      bones: []
      spriteID: {}
      internalID: {id}
      vertices: []
      indices:
      edges: []
      weights: []
",
            sprite_id(guid, i),
        ));
        table.push_str(&format!("      {name}: {id}\n"));
    }
    if sprites.is_empty() {
        sprites.push_str("    []\n");
    }
    let table = if table.is_empty() {
        " {}\n".to_string()
    } else {
        format!("\n{table}")
    };
    format!(
        "fileFormatVersion: 2
guid: {guid}
TextureImporter:
  internalIDToNameTable: []
  externalObjects: {{}}
  serializedVersion: 12
  mipmaps:
    mipMapMode: 0
    enableMipMap: 0
    sRGBTexture: 1
  isReadable: 0
  textureFormat: 1
  maxTextureSize: 16384
  textureSettings:
    serializedVersion: 2
    filterMode: 0
    aniso: 1
    mipBias: 0
    wrapU: 1
    wrapV: 1
    wrapW: 1
  nPOTScale: 0
  lightmap: 0
  compressionQuality: 50
  spriteMode: 2
  spriteExtrude: 1
  spriteMeshType: 0
  alignment: 0
  spritePivot: {{x: 0.5, y: 0.5}}
  spritePixelsToUnits: 100
  spriteBorder: {{x: 0, y: 0, z: 0, w: 0}}
  spriteGenerateFallbackPhysicsShape: 1
  alphaUsage: 1
  alphaIsTransparency: 1
  textureType: 8
  textureShape: 1
  platformSettings:
  - serializedVersion: 3
    buildTarget: DefaultTexturePlatform
    maxTextureSize: 16384
    resizeAlgorithm: 0
    textureFormat: -1
    textureCompression: 0
    compressionQuality: 50
    crunchedCompression: 0
    allowsAlphaSplitting: 0
    overridden: 0
  spriteSheet:
    serializedVersion: 2
    sprites:
{sprites}    outline: []
    physicsShape: []
    bones: []
    spriteID:
    internalID: 0
    vertices: []
    indices:
    edges: []
    weights: []
    secondaryTextures: []
    nameFileIdTable:{table}  spritePackingTag:
  pSDRemoveMatte: 0
  userData:
  assetBundleName:
  assetBundleVariant:
"
    )
}

/// The `guid:` of an existing `.meta`, so writing it again keeps Unity's
/// references to the texture.
fn existing_guid(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("guid:"))
        .map(str::trim)
        .filter(|guid| guid.len() == 32 && guid.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

/// 32 hex digits, different on each call.
fn new_guid(path: &Path) -> String {
    let random = RandomState::new();
    let mut first = random.build_hasher();
    first.write(path.to_string_lossy().as_bytes());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    first.write_u128(nanos);
    let a = first.finish();
    let mut second = random.build_hasher();
    second.write_u64(a);
    second.write_u32(std::process::id());
    format!("{a:016x}{:016x}", second.finish())
}

/// A sprite's ID, fixed for a texture and cell so exporting again keeps it.
fn sprite_id(guid: &str, cell: usize) -> String {
    // FNV-1a, two seeds, so the ID depends only on its inputs.
    let fnv = |seed: u64| {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
        for byte in guid.bytes().chain((cell as u64).to_le_bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    };
    format!("{:016x}{:016x}", fnv(1), fnv(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn tag(name: &str, from: usize, to: usize, direction: LoopDirection) -> FrameTag {
        FrameTag {
            name: name.into(),
            from,
            to,
            direction,
            repeat: 0,
        }
    }

    /// Four 2×1 frames of 100, 100, 200, 300 ms with tags walk 0-1 and
    /// jump 2-3 (ping-pong, played twice).
    fn doc() -> Document {
        let mut doc = Document::new(2, 1).unwrap();
        for _ in 0..3 {
            doc.add_frame().unwrap();
        }
        for (frame, ms) in [100, 100, 200, 300].into_iter().enumerate() {
            doc.set_frame_duration(frame, ms).unwrap();
            let shade = 50 * frame as u8;
            doc.set_pixel(frame, 0, 0, 0, Rgba::new(shade, 0, 0, 255))
                .unwrap();
        }
        doc.add_tag(tag("walk", 0, 1, LoopDirection::Forward))
            .unwrap();
        doc.add_tag(FrameTag {
            repeat: 2,
            ..tag("jump", 2, 3, LoopDirection::PingPong)
        })
        .unwrap();
        doc
    }

    #[test]
    fn play_order_follows_the_direction() {
        let order = |direction| play_order(&tag("t", 2, 5, direction));
        assert_eq!(order(LoopDirection::Forward), [2, 3, 4, 5]);
        assert_eq!(order(LoopDirection::Reverse), [5, 4, 3, 2]);
        assert_eq!(order(LoopDirection::PingPong), [2, 3, 4, 5, 4, 3]);
        assert_eq!(order(LoopDirection::PingPongReverse), [5, 4, 3, 2, 3, 4]);
        assert_eq!(play_order(&tag("t", 1, 2, LoopDirection::PingPong)), [1, 2]);
        assert_eq!(play_order(&tag("t", 3, 3, LoopDirection::PingPong)), [3]);
    }

    #[test]
    fn tag_paths_are_safe_and_distinct() {
        let tags = [
            tag("Walk Left", 0, 0, LoopDirection::Forward),
            tag("walk/left", 0, 0, LoopDirection::Forward),
            tag("../up", 0, 0, LoopDirection::Forward),
        ];
        let paths = tag_paths(Path::new("out/hero.png"), &tags);
        assert_eq!(
            paths,
            [
                PathBuf::from("out/hero_Walk_Left.png"),
                PathBuf::from("out/hero_walk_left_2.png"),
                PathBuf::from("out/hero____up.png"),
            ]
        );
    }

    #[test]
    fn godot_resource_has_one_animation_per_tag() {
        let doc = doc();
        let frames: Vec<usize> = (0..4).collect();
        let grid = sheet_grid(&doc, 4, SheetLayout::default()).unwrap();
        let tres = godot_sprite_frames(&doc, &frames, doc.tags(), &grid, "hero \"1\".png");
        assert!(tres.starts_with("[gd_resource type=\"SpriteFrames\" load_steps=6 format=3]"));
        assert!(tres.contains("path=\"hero \\\"1\\\".png\""));
        assert!(tres.contains("[sub_resource type=\"AtlasTexture\" id=\"AtlasTexture_3\"]\natlas = ExtResource(\"1_sheet\")\nregion = Rect2(6, 0, 2, 1)"));
        // walk: two 100 ms frames at 10 steps a second, looping.
        assert!(tres.contains("\"loop\": true,\n\"name\": &\"walk\",\n\"speed\": 10.0"));
        // jump: 200, 300 ms share 100 ms steps; repeat 2 does not loop.
        assert!(tres.contains("\"duration\": 2.0,\n\"texture\": SubResource(\"AtlasTexture_2\")"));
        assert!(tres.contains("\"duration\": 3.0,\n\"texture\": SubResource(\"AtlasTexture_3\")"));
        assert!(tres.contains("\"loop\": false,\n\"name\": &\"jump\""));
        // No tags: one looping default animation of every frame.
        let plain = godot_sprite_frames(&doc, &frames, &[], &grid, "a.png");
        assert_eq!(plain.matches("\"name\": &").count(), 1);
        assert!(plain.contains("\"name\": &\"default\""));
        assert_eq!(plain.matches("SubResource(\"AtlasTexture_").count(), 4);
    }

    #[test]
    fn unity_meta_slices_from_the_bottom_left() {
        let doc = doc();
        let layout = SheetLayout {
            columns: 2,
            padding: 1,
            ..SheetLayout::default()
        };
        let grid = sheet_grid(&doc, 4, layout).unwrap();
        let guid = "0123456789abcdef0123456789abcdef";
        let meta = unity_meta(&grid, "hero", guid);
        assert!(meta.starts_with("fileFormatVersion: 2\nguid: 0123456789abcdef0123456789abcdef\n"));
        assert!(meta.contains("  spriteMode: 2\n"));
        assert!(meta.contains("    filterMode: 0\n"));
        // The sheet is 5×3; frame 0 is top left, so its bottom edge is y 2.
        assert!(meta.contains("      name: hero_0\n      rect:\n        serializedVersion: 2\n        x: 0\n        y: 2\n"));
        assert!(meta.contains("      name: hero_3\n      rect:\n        serializedVersion: 2\n        x: 3\n        y: 0\n"));
        assert!(meta.contains("      hero_3: 21300006\n"));
        assert_eq!(existing_guid(&meta).as_deref(), Some(guid));
        assert_eq!(sprite_id(guid, 1), sprite_id(guid, 1));
        assert_ne!(sprite_id(guid, 1), sprite_id(guid, 2));
        assert_eq!(new_guid(Path::new("a")).len(), 32);
    }

    #[test]
    fn per_tag_exports_write_each_tag_and_keep_the_unity_guid() {
        let dir = std::env::temp_dir().join(format!("oxypixel-engine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = doc();
        let sheet = dir.join("hero.png");
        let files = SheetFiles {
            json: true,
            godot: true,
            unity: true,
        };
        let written =
            export_sheet_files(&doc, SheetLayout::default(), &sheet, files, true).unwrap();
        let names: Vec<String> = written
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "hero_walk.png",
                "hero_walk.json",
                "hero_walk.tres",
                "hero_walk.png.meta",
                "hero_jump.png",
                "hero_jump.json",
                "hero_jump.tres",
                "hero_jump.png.meta",
            ]
        );
        let json = std::fs::read_to_string(dir.join("hero_jump.json")).unwrap();
        assert!(json.contains("\"from\": 0, \"to\": 1, \"direction\": \"pingpong\""));
        let (w, _, _) =
            crate::io::decode_first_rgba(&std::fs::read(dir.join("hero_jump.png")).unwrap())
                .unwrap();
        assert_eq!(w, 4, "two 2-pixel frames");
        let meta = std::fs::read_to_string(dir.join("hero_jump.png.meta")).unwrap();
        let guid = existing_guid(&meta).unwrap();
        export_sheet_files(&doc, SheetLayout::default(), &sheet, files, true).unwrap();
        let again = std::fs::read_to_string(dir.join("hero_jump.png.meta")).unwrap();
        assert_eq!(
            existing_guid(&again),
            Some(guid),
            "re-exporting keeps the guid"
        );

        let gifs = export_gif_per_tag(&doc, &dir.join("hero.gif")).unwrap();
        assert_eq!(gifs, [dir.join("hero_walk.gif"), dir.join("hero_jump.gif")]);
        let jump = std::fs::read(dir.join("hero_jump.gif")).unwrap();
        // NETSCAPE2.0 loop count 1: the GIF plays twice.
        let at = jump.windows(11).position(|w| w == b"NETSCAPE2.0").unwrap();
        assert_eq!(&jump[at + 11..at + 15], &[3, 1, 1, 0]);
        let walk = std::fs::read(dir.join("hero_walk.gif")).unwrap();
        let at = walk.windows(11).position(|w| w == b"NETSCAPE2.0").unwrap();
        assert_eq!(
            &walk[at + 11..at + 15],
            &[3, 1, 0, 0],
            "repeat 0 loops forever"
        );

        let untagged = Document::new(2, 2).unwrap();
        assert!(export_gif_per_tag(&untagged, &dir.join("x.gif")).is_err());
        assert!(
            export_sheet_files(&untagged, SheetLayout::default(), &sheet, files, true).is_err()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
