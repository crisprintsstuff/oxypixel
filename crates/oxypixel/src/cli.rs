//! Headless commands. These do not open a window, so CI can run them without a GPU.

use std::path::Path;

use oxypixel_core::{
    export_bmp, export_gif, export_gif_per_tag, export_palette, export_png, export_sheet_files,
    import_palette, read_sheet, slice_sheet, Document, Error, SheetFiles, SheetLayout, SliceGrid,
    SliceInto, DEFAULT_FRAME_MS,
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
        "export-aseprite" => export_doc(args, |doc, path| {
            oxypack::save_aseprite(doc, path).map_err(Error::Io)
        }),
        "export-palette" => export_palette_cmd(args),
        "import-palette" => import_palette_cmd(args),
        "import-sheet" => import_sheet_cmd(args),
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
    match open(input) {
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
    match open(input) {
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
    match open(input) {
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
    match open(input) {
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
    let mut doc = match open(input) {
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

fn import_sheet_cmd(args: &[String]) -> u8 {
    let Some(input) = args.get(1) else {
        return usage("import-sheet needs an input image and an output .oxypix");
    };
    let Some(output) = args.get(2) else {
        return usage("import-sheet needs an output .oxypix");
    };
    let sheet = match read_sheet(Path::new(input)) {
        Ok(sheet) => sheet,
        Err(err) => return fail(&err.to_string()),
    };
    let mut grid = SliceGrid::guess(sheet.width, sheet.height);
    let (into, duration) = match parse_slice_options(&args[3..], &mut grid) {
        Ok(options) => options,
        Err(message) => return usage(&message),
    };
    let doc = match slice_sheet(&sheet, &grid, into, duration) {
        Ok(doc) => doc,
        Err(err) => return fail(&err.to_string()),
    };
    match oxypixel_core::save_oxypix(&doc, Path::new(output)) {
        Ok(()) => 0,
        Err(err) => fail(&err.to_string()),
    }
}

/// `--cell WxH`, `--offset X,Y`, `--gap X,Y`, `--layers`, `--keep-empty`,
/// and `--duration MS`, in any order. Returns what the cells become and how
/// long each frame lasts.
fn parse_slice_options(args: &[String], grid: &mut SliceGrid) -> Result<(SliceInto, u32), String> {
    let mut into = SliceInto::Frames;
    let mut duration = DEFAULT_FRAME_MS;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let mut pair = |name: &str, sep: char| -> Result<(u32, u32), String> {
            rest.next()
                .and_then(|value| value.split_once(sep))
                .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))
                .ok_or_else(|| format!("{name} needs two whole numbers, like 16{sep}16"))
        };
        match flag.as_str() {
            "--cell" => (grid.cell_width, grid.cell_height) = pair("--cell", 'x')?,
            "--offset" => (grid.offset_x, grid.offset_y) = pair("--offset", ',')?,
            "--gap" => (grid.gap_x, grid.gap_y) = pair("--gap", ',')?,
            "--layers" => into = SliceInto::Layers,
            "--keep-empty" => grid.keep_empty = true,
            "--duration" => {
                duration = rest
                    .next()
                    .and_then(|value| value.parse::<u32>().ok())
                    .ok_or("--duration needs a whole number of milliseconds")?;
            }
            other => return Err(format!("unknown import-sheet option {other}")),
        }
    }
    Ok((into, duration))
}

/// An `.oxypix` document, or an `.aseprite` file read as one. Notes on what
/// an Aseprite file had that could not be kept go to stderr.
fn open(input: &str) -> Result<Document, String> {
    let (doc, notes) = oxypack::open_document(Path::new(input))?;
    for note in notes {
        eprintln!("oxypixel: note: {note}");
    }
    Ok(doc)
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
  oxypixel export-aseprite INPUT.oxypix OUTPUT.aseprite
  oxypixel export-palette INPUT.oxypix OUTPUT.gpl
  oxypixel import-palette INPUT.oxypix PALETTE.gpl OUTPUT.oxypix
  oxypixel import-sheet SHEET.png OUTPUT.oxypix [--cell WxH] [--offset X,Y] [--gap X,Y]
                        [--layers] [--keep-empty] [--duration MS]
  oxypixel mcp [--root DIR]
  oxypixel --help
  oxypixel --version

With no command, OxyPixel opens the editor. A file argument opens that .oxypix document.
Export commands do not open a window. FRAME is the zero-based frame index and defaults to 0.
An INPUT.oxypix may also be an .aseprite or .ase file, read with its layers, tags, and palette.
export-aseprite writes an Aseprite file with the layers, groups, blend modes, tags, palette, and
tilemaps.
export-palette writes the document palette as GIMP .gpl, hex .hex, or JASC .pal.
import-palette replaces that palette and writes a new .oxypix. A .txt palette is read as hex.
export-sheet puts frames in one row unless --columns is set. --padding leaves clear pixels between
cells, and --json writes OUTPUT.json with each frame's rectangle and duration. --godot writes
OUTPUT.tres, a Godot 4 SpriteFrames resource with one animation per tag, and --unity writes
OUTPUT.png.meta, which Unity reads as one sprite per frame. --per-tag writes OUTPUT_<tag>.png
(or .gif for export-gif) for each tag instead of one file.
import-sheet cuts a PNG, BMP, or GIF spritesheet into cells, row by row, and writes one frame
per cell (or one layer per cell with --layers). The cell size defaults to square cells for a
one-row or one-column strip, or the whole image. --offset skips pixels at the top-left corner and
--gap skips pixels between cells. Empty cells are dropped unless --keep-empty is given.
mcp runs a Model Context Protocol server on stdin and stdout for AI assistants; with --root,
its tools may only read and write inside DIR.
"
);
