//! Headless commands. These do not open a window, so CI can run them without a GPU.

use std::path::Path;

use oxypixel_core::{export_bmp, export_gif, export_png, export_spritesheet, Document, Error};

pub fn dispatch(args: &[String]) -> Option<i32> {
    let command = args.first().map(String::as_str)?;
    let code = match command {
        "--help" | "-h" | "help" => {
            print!("{HELP}");
            0
        }
        "--version" | "-V" | "version" => {
            println!("oxypixel {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "export-png" => export_frame(args, export_png),
        "export-bmp" => export_frame(args, export_bmp),
        "export-gif" => export_doc(args, export_gif),
        "export-sheet" => export_doc(args, export_spritesheet),
        _ => return None,
    };
    Some(i32::from(code))
}

fn export_frame(args: &[String], write: fn(&Document, usize, &Path) -> Result<(), Error>) -> u8 {
    let Some(input) = args.get(1) else {
        return usage("export needs an input .oxypix and an output path");
    };
    let Some(output) = args.get(2) else {
        return usage("export needs an output path");
    };
    let frame = match args.get(3) {
        None => 0,
        Some(text) => match text.parse::<usize>() {
            Ok(index) => index,
            Err(_) => return usage("the frame index must be a number"),
        },
    };
    match oxypixel_core::load_oxypix(Path::new(input)) {
        Ok(doc) => match write(&doc, frame, Path::new(output)) {
            Ok(()) => 0,
            Err(err) => fail(&err.to_string()),
        },
        Err(err) => fail(&err.to_string()),
    }
}

fn export_doc(args: &[String], write: fn(&Document, &Path) -> Result<(), Error>) -> u8 {
    let Some(input) = args.get(1) else {
        return usage("export needs an input .oxypix and an output path");
    };
    let Some(output) = args.get(2) else {
        return usage("export needs an output path");
    };
    match oxypixel_core::load_oxypix(Path::new(input)) {
        Ok(doc) => match write(&doc, Path::new(output)) {
            Ok(()) => 0,
            Err(err) => fail(&err.to_string()),
        },
        Err(err) => fail(&err.to_string()),
    }
}

fn usage(message: &str) -> u8 {
    eprintln!("oxypixel: {message}");
    eprintln!("Run `oxypixel --help` for usage.");
    2
}

/// Report a failed export and return exit code 1.
///
/// This must not open a native dialog. `rfd`'s Windows backend calls
/// `MessageBoxW`, which does not return until a button is clicked. On a
/// headless runner nothing clicks it, so `cargo test` waits until the job
/// timeout. The message is already on stderr for scripts and for the tests.
fn fail(message: &str) -> u8 {
    eprintln!("oxypixel: {message}");
    1
}

pub const HELP: &str = "\
OxyPixel 0.1.0, a pixel-art editor

Usage:
  oxypixel [FILE]
  oxypixel export-png INPUT.oxypix OUTPUT.png [FRAME]
  oxypixel export-bmp INPUT.oxypix OUTPUT.bmp [FRAME]
  oxypixel export-gif INPUT.oxypix OUTPUT.gif
  oxypixel export-sheet INPUT.oxypix OUTPUT.png
  oxypixel --help
  oxypixel --version

With no command, OxyPixel opens the editor. A file argument opens that .oxypix document.
Export commands do not open a window. FRAME is the zero-based frame index and defaults to 0.
";
