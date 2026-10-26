//! Gather input files, pack them, and write the atlas.
//!
//! The command line and the editor's Pack sprites window both go through
//! [`pack_files`] and [`write_atlas`], so they make the same atlas from the
//! same inputs.

use std::io::Read;
use std::path::{Path, PathBuf};

use oxypixel_core::{import_image, load_oxypix};

use crate::aseprite;
use crate::pack::{
    atlas_json, atlas_png_bytes, frame_tags, pack_sprites, sprites_from_aseprite,
    sprites_from_document, unique_names, Atlas, FrameTag, PackOptions, Sprite,
};

/// Whether OxyPack reads this file, by its extension.
pub fn packable(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "bmp" | "gif" | "oxypix" | "aseprite" | "ase"
    )
}

/// The files to pack, in command-line order. A folder adds its packable
/// files sorted by name.
pub fn collect(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let entries =
                std::fs::read_dir(input).map_err(|err| format!("{}: {err}", input.display()))?;
            let mut found: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_file() && packable(path))
                .collect();
            found.sort();
            files.extend(found);
        } else if packable(input) {
            files.push(input.clone());
        } else {
            return Err(format!(
                "{}: not a .png, .bmp, .gif, .oxypix, or .aseprite file",
                input.display()
            ));
        }
    }
    Ok(files)
}

/// One input file, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub sprites: Vec<Sprite>,
    /// Animation tags, counted from this file's first frame.
    pub tags: Vec<aseprite::Tag>,
    /// What could not be drawn exactly as Aseprite would, one line each.
    pub notes: Vec<String>,
}

/// The sprites in one file, with its tags and notes.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "aseprite" || ext == "ase" {
        let bytes = read_limited(path)?;
        let file =
            aseprite::read_aseprite(&bytes).map_err(|err| format!("{}: {err}", path.display()))?;
        return Ok(Loaded {
            sprites: sprites_from_aseprite(&file, &stem),
            tags: file.tags,
            notes: file.notes,
        });
    }
    let is_doc = ext == "oxypix";
    let doc = if is_doc {
        load_oxypix(path)
    } else {
        import_image(path)
    }
    .map_err(|err| format!("{}: {err}", path.display()))?;
    let sprites =
        sprites_from_document(&doc, &stem).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(Loaded {
        sprites,
        tags: Vec::new(),
        notes: Vec::new(),
    })
}

/// Read a file no larger than the core's file cap.
fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let mut bytes = Vec::new();
    std::io::Read::take(file, oxypixel_core::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(bytes)
}

/// A packed atlas and what went into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packed {
    pub atlas: Atlas,
    pub tags: Vec<FrameTag>,
    /// How many files the inputs named, after folders were opened.
    pub file_count: usize,
    /// Notes from the files, each starting with the file's path.
    pub notes: Vec<String>,
}

/// Collect, load, name, and pack the inputs. Nothing is written.
pub fn pack_files(inputs: &[PathBuf], options: PackOptions) -> Result<Packed, String> {
    let files = collect(inputs)?;
    let mut sprites = Vec::new();
    let mut tags = Vec::new();
    let mut notes = Vec::new();
    for file in &files {
        let loaded = load(file)?;
        tags.extend(frame_tags(&loaded.tags, sprites.len()));
        sprites.extend(loaded.sprites);
        notes.extend(
            loaded
                .notes
                .into_iter()
                .map(|note| format!("{}: {note}", file.display())),
        );
    }
    unique_names(&mut sprites);
    let atlas = pack_sprites(&sprites, options).map_err(|err| err.to_string())?;
    Ok(Packed {
        atlas,
        tags,
        file_count: files.len(),
        notes,
    })
}

/// Write the atlas PNG to `output`, and the JSON beside it with the same name
/// when `json` is set.
pub fn write_atlas(packed: &Packed, output: &Path, json: bool) -> Result<(), String> {
    let png = atlas_png_bytes(&packed.atlas).map_err(|err| err.to_string())?;
    let image = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::fs::write(output, png).map_err(|err| format!("{}: {err}", output.display()))?;
    if json {
        let path = output.with_extension("json");
        std::fs::write(&path, atlas_json(&packed.atlas, &image, &packed.tags))
            .map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aseprite_frames_and_tags_follow_the_files_before_them() {
        use aseprite::tests::{cel, file, layer, tags};
        let dir = std::env::temp_dir().join(format!("oxypack-tags-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = oxypixel_core::Document::new(2, 2).unwrap();
        std::fs::write(
            dir.join("a coin.png"),
            oxypixel_core::export_png_bytes(&doc, 0).unwrap(),
        )
        .unwrap();
        let red = [255, 0, 0, 255];
        let ase = file(
            1,
            1,
            32,
            0,
            vec![
                (
                    90,
                    vec![
                        layer(1, 0, 0, 0, 255, "art"),
                        cel(0, 0, 0, 255, 1, 1, &red),
                        tags(&[(1, 2, 0, 0, "walk")]),
                    ],
                ),
                (90, vec![cel(0, 0, 0, 255, 1, 1, &red)]),
                (40, vec![]),
            ],
        );
        std::fs::write(dir.join("b hero.aseprite"), ase).unwrap();
        let packed = pack_files(std::slice::from_ref(&dir), PackOptions::default()).unwrap();
        assert_eq!((packed.atlas.placements.len(), packed.file_count), (4, 2));
        write_atlas(&packed, &dir.join("atlas.png"), true).unwrap();
        let json = std::fs::read_to_string(dir.join("atlas.json")).unwrap();
        for name in ["\"a coin\"", "\"b hero 0\"", "\"b hero 2\""] {
            assert!(json.contains(name), "{json}");
        }
        assert!(json.contains("\"duration\": 40"));
        // The coin is sprite 0, so the hero's frames 1 to 2 are sprites 2 to 3.
        assert!(
            json.contains("\"frameTags\": [{\"name\": \"walk\", \"from\": 2, \"to\": 3, \"direction\": \"forward\"}]"),
            "{json}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_image_and_document_files_are_packable() {
        assert!(packable(Path::new("a.PNG")));
        assert!(packable(Path::new("walk.oxypix")));
        assert!(packable(Path::new("x.gif")));
        assert!(packable(Path::new("hero.aseprite")));
        assert!(packable(Path::new("hero.ASE")));
        assert!(!packable(Path::new("photo.jpg")));
        assert!(!packable(Path::new("notes")));
        assert!(collect(&[PathBuf::from("photo.jpg")]).is_err());
    }
}
