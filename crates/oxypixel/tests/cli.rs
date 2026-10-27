//! Runs the real binary so export commands stay wired to the library.

use std::process::Command;

use oxypixel_core::{save_oxypix, Editor, MouseButton, Rgba};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_oxypixel"))
}

#[test]
fn help_lists_export() {
    let output = bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("export-png"));
    assert!(text.contains("export-gif"));
    assert!(text.contains("export-palette"));
    assert!(text.contains("import-palette"));
}

#[test]
fn export_sheet_takes_columns_padding_and_writes_json() {
    let dir = std::env::temp_dir().join(format!("oxypixel-cli-sheet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("walk.oxypix");
    let mut editor = Editor::new(2, 2).unwrap();
    editor.add_frame().unwrap();
    editor.add_frame().unwrap();
    save_oxypix(editor.document(), &src).unwrap();
    let png = dir.join("walk.png");
    let output = bin()
        .args(["export-sheet", src.to_str().unwrap(), png.to_str().unwrap()])
        .args(["--columns", "2", "--padding", "1", "--scale", "2", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let image = image::ImageReader::open(&png).unwrap().decode().unwrap();
    // Two 4-pixel columns and a 1-pixel gap; two rows.
    assert_eq!((image.width(), image.height()), (9, 9));
    let json = std::fs::read_to_string(dir.join("walk.json")).unwrap();
    assert_eq!(json.matches("\"filename\"").count(), 3);
    assert!(json.contains("\"image\": \"walk.png\""));

    let bad = bin()
        .args(["export-sheet", src.to_str().unwrap(), png.to_str().unwrap()])
        .args(["--columns", "two"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    let unknown = bin()
        .args([
            "export-sheet",
            src.to_str().unwrap(),
            png.to_str().unwrap(),
            "--rows",
        ])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(2));
    let too_wide = bin()
        .args(["export-sheet", src.to_str().unwrap(), png.to_str().unwrap()])
        .args(["--padding", "9999"])
        .output()
        .unwrap();
    assert_eq!(too_wide.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_files_and_per_tag_exports() {
    let dir = std::env::temp_dir().join(format!("oxypixel-cli-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("hero.oxypix");
    let mut doc = oxypixel_core::Document::new(2, 2).unwrap();
    doc.add_frame().unwrap();
    doc.add_frame().unwrap();
    doc.add_tag(oxypixel_core::FrameTag {
        name: "idle".into(),
        from: 0,
        to: 1,
        direction: oxypixel_core::LoopDirection::Forward,
        repeat: 0,
    })
    .unwrap();
    save_oxypix(&doc, &src).unwrap();
    let png = dir.join("hero.png");
    let output = bin()
        .args(["export-sheet", src.to_str().unwrap(), png.to_str().unwrap()])
        .args(["--godot", "--unity"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tres = std::fs::read_to_string(dir.join("hero.tres")).unwrap();
    assert!(tres.contains("\"name\": &\"idle\""));
    assert!(std::fs::read_to_string(dir.join("hero.png.meta"))
        .unwrap()
        .contains("name: hero_2"));
    let output = bin()
        .args([
            "export-sheet",
            src.to_str().unwrap(),
            png.to_str().unwrap(),
            "--per-tag",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let image = image::ImageReader::open(dir.join("hero_idle.png"))
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(image.width(), 4, "the idle tag has two frames");
    let gif = dir.join("hero.gif");
    let output = bin()
        .args([
            "export-gif",
            src.to_str().unwrap(),
            gif.to_str().unwrap(),
            "--per-tag",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(dir.join("hero_idle.gif").exists());
    assert!(!gif.exists());
    let bad = bin()
        .args([
            "export-gif",
            src.to_str().unwrap(),
            gif.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 2×1 Aseprite file with one visible layer and one red pixel at (1, 0),
/// written from the published format.
fn tiny_aseprite() -> Vec<u8> {
    let chunk = |kind: u16, data: Vec<u8>| {
        let mut out = (data.len() as u32 + 6).to_le_bytes().to_vec();
        out.extend(kind.to_le_bytes());
        out.extend(data);
        out
    };
    let mut layer = Vec::new();
    for v in [1u16, 0, 0, 0, 0, 0] {
        layer.extend(v.to_le_bytes()); // visible, image, level 0, Normal
    }
    layer.extend([255, 0, 0, 0]);
    layer.extend(3u16.to_le_bytes());
    layer.extend(b"ink");
    let mut cel = Vec::new();
    cel.extend(0u16.to_le_bytes()); // layer
    cel.extend(1i16.to_le_bytes()); // x
    cel.extend(0i16.to_le_bytes()); // y
    cel.push(255);
    cel.extend(0u16.to_le_bytes()); // raw
    cel.extend([0; 7]);
    cel.extend(1u16.to_le_bytes());
    cel.extend(1u16.to_le_bytes());
    cel.extend([255, 0, 0, 255]);
    let mut chunks = chunk(0x2004, layer);
    chunks.extend(chunk(0x2005, cel));
    let mut frame = (chunks.len() as u32 + 16).to_le_bytes().to_vec();
    frame.extend(0xF1FAu16.to_le_bytes());
    frame.extend(2u16.to_le_bytes());
    frame.extend(100u16.to_le_bytes());
    frame.extend([0; 2]);
    frame.extend(2u32.to_le_bytes());
    frame.extend(chunks);
    let mut out = (frame.len() as u32 + 128).to_le_bytes().to_vec();
    out.extend(0xA5E0u16.to_le_bytes());
    for v in [1u16, 2, 1, 32] {
        out.extend(v.to_le_bytes()); // frames, width, height, depth
    }
    out.extend(1u32.to_le_bytes()); // layer opacity is valid
    out.resize(128, 0);
    out.extend(frame);
    out
}

#[test]
fn exports_read_aseprite_files() {
    let dir = std::env::temp_dir().join(format!("oxypixel-cli-ase-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("dot.aseprite");
    std::fs::write(&src, tiny_aseprite()).unwrap();
    let png = dir.join("dot.png");
    let output = bin()
        .args(["export-png", src.to_str().unwrap(), png.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let image = image::ImageReader::open(&png)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();
    assert_eq!(image.dimensions(), (2, 1));
    assert_eq!(image.get_pixel(1, 0).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(0, 0).0[3], 0);
    let broken = dir.join("broken.ase");
    std::fs::write(&broken, b"not aseprite").unwrap();
    let output = bin()
        .args([
            "export-png",
            broken.to_str().unwrap(),
            png.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_png_writes_the_painted_pixel() {
    let dir = std::env::temp_dir().join(format!(
        "oxypixel-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("sprite.oxypix");
    let png = dir.join("sprite.png");
    let mut editor = Editor::new(2, 2).unwrap();
    editor.set_primary(Rgba::new(10, 20, 30, 255));
    editor.pointer_down(0, 0, MouseButton::Primary, false);
    editor.pointer_up(0, 0, false);
    save_oxypix(editor.document(), &src).unwrap();

    let output = bin()
        .args(["export-png", src.to_str().unwrap(), png.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let image = image::ImageReader::open(&png)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();
    assert_eq!(image.dimensions(), (2, 2));
    assert_eq!(image.get_pixel(0, 0).0, [10, 20, 30, 255]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_fails() {
    let mut child = bin()
        .args(["export-png", "this-file-does-not-exist.oxypix", "out.png"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // A Windows message box never returns on a headless runner. Fail the
    // test instead of waiting for the job timeout.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("export-png on a missing file hung instead of exiting");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("file error"),
        "stderr should report the missing file, got {err}"
    );
}

#[test]
fn export_palette_missing_file_exits() {
    let mut child = bin()
        .args([
            "export-palette",
            "this-file-does-not-exist.oxypix",
            "out.gpl",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("export-palette on a missing file hung instead of exiting");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("file error"),
        "stderr should report the missing file, got {err}"
    );
}

#[test]
fn palette_export_and_import_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "oxypixel-cli-palette-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("sprite.oxypix");
    let gpl = dir.join("ink.gpl");
    let out = dir.join("recolored.oxypix");
    let mut editor = Editor::new(2, 2).unwrap();
    editor.set_primary(Rgba::new(10, 20, 30, 255));
    editor.pointer_down(0, 0, MouseButton::Primary, false);
    editor.pointer_up(0, 0, false);
    editor
        .set_palette(vec![Rgba::new(1, 2, 3, 255), Rgba::new(4, 5, 6, 128)])
        .unwrap();
    save_oxypix(editor.document(), &src).unwrap();

    let exported = bin()
        .args([
            "export-palette",
            src.to_str().unwrap(),
            gpl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let text = std::fs::read_to_string(&gpl).unwrap();
    assert!(text.starts_with("GIMP Palette"));
    assert!(text.contains("1 2 3"));
    assert!(text.contains("4 5 6 128"));

    let imported = bin()
        .args([
            "import-palette",
            src.to_str().unwrap(),
            gpl.to_str().unwrap(),
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        imported.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let doc = oxypixel_core::load_oxypix(&out).unwrap();
    assert_eq!(
        doc.palette(),
        &[Rgba::new(1, 2, 3, 255), Rgba::new(4, 5, 6, 128)]
    );
    assert_eq!(doc.composite(0).unwrap()[0], Rgba::new(10, 20, 30, 255));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_does_not_write_a_recovery_file() {
    let root = std::env::temp_dir().join(format!(
        "oxypixel-cli-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("sprite.oxypix");
    let png = root.join("sprite.png");
    let mut editor = Editor::new(2, 2).unwrap();
    editor.set_primary(Rgba::new(10, 20, 30, 255));
    editor.pointer_down(0, 0, MouseButton::Primary, false);
    editor.pointer_up(0, 0, false);
    save_oxypix(editor.document(), &src).unwrap();

    #[cfg(windows)]
    let data = root.join("OxyPixel");
    #[cfg(not(windows))]
    let data = root.join("oxypixel");

    let mut command = bin();
    #[cfg(windows)]
    command.env("APPDATA", &root);
    #[cfg(not(windows))]
    command.env("XDG_DATA_HOME", &root);
    command
        .arg("export-png")
        .arg(&src)
        .arg(&png)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("export-png hung instead of exiting");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(png.is_file());
    assert!(
        !data.join("recovery").exists(),
        "export must not write a recovery file"
    );
    assert!(!data.join("autosave.txt").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn mcp_server_answers_over_stdio_and_exits_at_end_of_input() {
    use std::io::{Read, Write};
    use std::process::Stdio;

    let dir = std::env::temp_dir().join(format!("oxypixel-cli-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = bin()
        .args(["mcp", "--root", dir.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"create_document","arguments":{"path":"a.oxypix","width":4,"height":4}}}"#,
        "\n",
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("oxypixel mcp did not exit after its input ended");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(status.success());
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let replies: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("each stdout line is JSON-RPC"))
        .collect();
    assert_eq!(replies.len(), 3, "{stdout}");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "oxypixel");
    assert!(replies[1]["result"]["tools"].as_array().unwrap().len() >= 10);
    assert_eq!(replies[2]["result"]["isError"], false, "{stdout}");
    assert!(dir.join("a.oxypix").is_file());

    let bad = bin()
        .args(["mcp", "--root", dir.join("missing").to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}
