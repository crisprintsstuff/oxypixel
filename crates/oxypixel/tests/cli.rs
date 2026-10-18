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
