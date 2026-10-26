//! Runs the real binary on files in a temp folder.

use std::process::Command;

use oxypixel_core::{save_oxypix, Editor, MouseButton, Rgba};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_oxypack"))
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oxypack-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `w`×`h` document with one inked pixel at `(x, y)` on each of `frames` frames.
fn doc(w: u32, h: u32, frames: usize, x: i32, y: i32) -> Editor {
    let mut editor = Editor::new(w, h).unwrap();
    editor.set_primary(Rgba::new(200, 40, 40, 255));
    for frame in 0..frames {
        if frame > 0 {
            editor.add_frame().unwrap();
        }
        editor.pointer_down(x, y, MouseButton::Primary, false);
        editor.pointer_up(x, y, false);
    }
    editor
}

#[test]
fn packs_a_folder_and_a_document_into_a_png_and_json() {
    let dir = temp_dir("pack");
    let sprites = dir.join("sprites");
    std::fs::create_dir_all(&sprites).unwrap();
    save_oxypix(doc(4, 4, 2, 1, 1).document(), &dir.join("walk.oxypix")).unwrap();
    std::fs::write(
        sprites.join("coin.png"),
        oxypixel_core::export_png_bytes(doc(3, 3, 1, 0, 0).document(), 0).unwrap(),
    )
    .unwrap();
    std::fs::write(sprites.join("readme.txt"), "not a sprite").unwrap();
    let out = dir.join("atlas.png");
    let output = bin()
        .arg(dir.join("walk.oxypix"))
        .arg(&sprites)
        .args(["--trim", "-o"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary = String::from_utf8_lossy(&output.stdout);
    assert!(
        summary.contains("packed 3 sprites from 2 files"),
        "{summary}"
    );
    let png = std::fs::read(&out).unwrap();
    let atlas = oxypixel_core::import_image_bytes(&png).unwrap();
    // Three trimmed one-pixel sprites with one pixel of padding between them.
    assert_eq!((atlas.width(), atlas.height()), (5, 1));
    let json = std::fs::read_to_string(dir.join("atlas.json")).unwrap();
    for name in ["\"walk 0\"", "\"walk 1\"", "\"coin\""] {
        assert!(json.contains(name), "{json}");
    }
    assert!(json.contains("\"spriteSourceSize\": {\"x\": 1, \"y\": 1, \"w\": 1, \"h\": 1}"));
    assert!(json.contains("\"image\": \"atlas.png\""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_exit_without_writing() {
    let dir = temp_dir("errors");
    let out = dir.join("atlas.png");
    let missing = bin()
        .arg(dir.join("nope.png"))
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("nope.png"));
    let usage = bin().arg("--frobnicate").output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(!out.exists());
    let help = bin().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--dedupe"));
    let _ = std::fs::remove_dir_all(&dir);
}
