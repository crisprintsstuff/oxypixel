//! Headless commands. These do not open a window, so CI can run them without a GPU.

use std::path::Path;

use oxypixel_core::{
    export_bmp, export_gif, export_gif_per_tag, export_palette, export_png, export_sheet_files,
    import_palette, Document, Error, SheetFiles, SheetLayout,
};

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
        "export-gif" => export_gif_cmd(args),
        "export-sheet" => export_sheet_cmd(args),
        "export-palette" => export_palette_cmd(args),
        "import-palette" => import_palette_cmd(args),
        "mcp" => crate::mcp::run(&args[1..]),
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

fn export_gif_cmd(args: &[String]) -> u8 {
    match args.get(3).map(String::as_str) {
        None => export_doc(args, export_gif),
        Some("--per-tag") if args.len() == 4 => {
            export_doc(args, |doc, path| export_gif_per_tag(doc, path).map(|_| ()))
        }
        Some(other) => usage(&format!("unknown export-gif option {other}")),
    }
}

fn export_sheet_cmd(args: &[String]) -> u8 {
    let Some(input) = args.get(1) else {
        return usage("export-sheet needs an input .oxypix and an output .png");
    };
    let Some(output) = args.get(2) else {
        return usage("export-sheet needs an output .png");
    };
    let (layout, files, per_tag) = match parse_sheet_options(&args[3..]) {
        Ok(options) => options,
        Err(message) => return usage(&message),
    };
    match oxypixel_core::load_oxypix(Path::new(input)) {
        Ok(doc) => match export_sheet_files(&doc, layout, Path::new(output), files, per_tag) {
            Ok(_) => 0,
            Err(err) => fail(&err.to_string()),
        },
        Err(err) => fail(&err.to_string()),
    }
}

/// `--columns N`, `--padding N`, `--scale 1|2|4`, `--json`, `--godot`,
/// `--unity`, and `--per-tag`, in any order.
fn parse_sheet_options(args: &[String]) -> Result<(SheetLayout, SheetFiles, bool), String> {
    let mut layout = SheetLayout::default();
    let mut files = SheetFiles::default();
    let mut per_tag = false;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let mut number = |name: &str| -> Result<u32, String> {
            rest.next()
                .and_then(|value| value.parse::<u32>().ok())
                .ok_or_else(|| format!("{name} needs a whole number"))
        };
        match flag.as_str() {
            "--columns" => layout.columns = number("--columns")?,
            "--padding" => layout.padding = number("--padding")?,
            "--scale" => layout.scale = number("--scale")?,
            "--json" => files.json = true,
            "--godot" => files.godot = true,
            "--unity" => files.unity = true,
            "--per-tag" => per_tag = true,
            other => return Err(format!("unknown export-sheet option {other}")),
        }
    }
    Ok((layout, files, per_tag))
}

fn export_palette_cmd(args: &[String]) -> u8 {
    let Some(input) = args.get(1) else {
        return usage("export-palette needs an input .oxypix and an output palette");
    };
    let Some(output) = args.get(2) else {
        return usage("export-palette needs an output .gpl, .hex, or .pal path");
    };
    match oxypixel_core::load_oxypix(Path::new(input)) {
        Ok(doc) => match export_palette(doc.palette(), Path::new(output)) {
            Ok(()) => 0,
            Err(err) => fail(&err.to_string()),
        },
        Err(err) => fail(&err.to_string()),
    }
}

fn import_palette_cmd(args: &[String]) -> u8 {
    let Some(input) = args.get(1) else {
        return usage(
            "import-palette needs an input .oxypix, a palette file, and an output .oxypix",
        );
    };
    let Some(palette) = args.get(2) else {
        return usage("import-palette needs a palette file and an output .oxypix");
    };
    let Some(output) = args.get(3) else {
        return usage("import-palette needs an output .oxypix");
    };
    let mut doc = match oxypixel_core::load_oxypix(Path::new(input)) {
        Ok(doc) => doc,
        Err(err) => return fail(&err.to_string()),
    };
    let colors = match import_palette(Path::new(palette)) {
        Ok(colors) => colors,
        Err(err) => return fail(&err.to_string()),
    };
    if let Err(err) = doc.set_palette(colors) {
        return fail(&err.to_string());
    }
    match oxypixel_core::save_oxypix(&doc, Path::new(output)) {
        Ok(()) => 0,
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

pub const HELP: &str = concat!(
    "OxyPixel ",
    env!("CARGO_PKG_VERSION"),
    ", a pixel-art editor

Usage:
  oxypixel [FILE]
  oxypixel export-png INPUT.oxypix OUTPUT.png [FRAME]
  oxypixel export-bmp INPUT.oxypix OUTPUT.bmp [FRAME]
  oxypixel export-gif INPUT.oxypix OUTPUT.gif [--per-tag]
  oxypixel export-sheet INPUT.oxypix OUTPUT.png [--columns N] [--padding N] [--scale 1|2|4]
                        [--json] [--godot] [--unity] [--per-tag]
  oxypixel export-palette INPUT.oxypix OUTPUT.gpl
  oxypixel import-palette INPUT.oxypix PALETTE.gpl OUTPUT.oxypix
  oxypixel mcp [--root DIR]
  oxypixel --help
  oxypixel --version

With no command, OxyPixel opens the editor. A file argument opens that .oxypix document.
Export commands do not open a window. FRAME is the zero-based frame index and defaults to 0.
export-palette writes the document palette as GIMP .gpl, hex .hex, or JASC .pal.
import-palette replaces that palette and writes a new .oxypix. A .txt palette is read as hex.
export-sheet puts frames in one row unless --columns is set. --padding leaves clear pixels between
cells, and --json writes OUTPUT.json with each frame's rectangle and duration. --godot writes
OUTPUT.tres, a Godot 4 SpriteFrames resource with one animation per tag, and --unity writes
OUTPUT.png.meta, which Unity reads as one sprite per frame. --per-tag writes OUTPUT_<tag>.png
(or .gif for export-gif) for each tag instead of one file.
mcp runs a Model Context Protocol server on stdin and stdout for AI assistants; with --root,
its tools may only read and write inside DIR.
"
);
