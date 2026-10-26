//! OxyPack: pack sprites from many files into one atlas PNG and a JSON hash.
//!
//! The packing and the JSON are in `pack.rs`; this file reads the command
//! line, gathers the inputs, and writes the two files. It never opens
//! a window or a dialog, so it is safe in build scripts and CI.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oxypixel_core::{import_image, load_oxypix};

mod aseprite;
mod pack;

use pack::{
    atlas_json, atlas_png_bytes, pack_sprites, sprites_from_aseprite, sprites_from_document,
    unique_names, PackOptions, Packer, Sprite,
};

const HELP: &str = concat!(
    "OxyPack ",
    env!("CARGO_PKG_VERSION"),
    ", a sprite atlas packer from the OxyPixel project

Usage: oxypack [options] <input>... -o <atlas.png>

Inputs are .png, .bmp, .gif, .oxypix, and .aseprite (.ase) files, or
folders of them (not searched recursively). Each frame of an animation is its own
sprite, named `<file> 0`, `<file> 1`, and so on; a one-frame file is named
after the file. The atlas PNG is written to -o and the JSON hash beside it.

Options:
  -o, --output <path>   Atlas PNG to write (required)
  --padding <n>         Clear pixels between sprites (default 1, up to 256)
  --max-width <n>       Widest the atlas may be (default 2048, up to 16384)
  --trim                Crop each sprite to its non-clear pixels
  --dedupe              Store sprites with the same pixels once
  --pot                 Round the atlas size up to powers of two
  --packer <name>       shelf (default) or maxrects, which fills the gaps
                        rows leave and usually gives a shorter atlas
  --no-json             Write only the PNG
  -h, --help            Show this help
  -V, --version         Show the version
"
);

#[derive(Debug, PartialEq, Eq)]
struct Args {
    inputs: Vec<PathBuf>,
    output: PathBuf,
    options: PackOptions,
    json: bool,
}

enum Parsed {
    Run(Args),
    Help,
    Version,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut inputs = Vec::new();
    let mut output = None;
    let mut options = PackOptions::default();
    let mut json = true;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut number = |name: &str| -> Result<u32, String> {
            rest.next()
                .and_then(|value| value.parse::<u32>().ok())
                .ok_or_else(|| format!("{name} needs a whole number"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "-o" | "--output" => {
                output = Some(PathBuf::from(
                    rest.next().ok_or("-o needs the atlas .png path")?,
                ))
            }
            "--padding" => options.padding = number("--padding")?,
            "--max-width" => options.max_width = number("--max-width")?,
            "--trim" => options.trim = true,
            "--dedupe" => options.dedupe = true,
            "--pot" => options.power_of_two = true,
            "--packer" => {
                options.packer = match rest.next().map(String::as_str) {
                    Some("shelf") => Packer::Shelf,
                    Some("maxrects") => Packer::MaxRects,
                    _ => return Err("--packer takes shelf or maxrects".into()),
                }
            }
            "--no-json" => json = false,
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Err(format!("unknown option {flag}"))
            }
            path => inputs.push(PathBuf::from(path)),
        }
    }
    let output = output.ok_or("-o <atlas.png> is required")?;
    if inputs.is_empty() {
        return Err("give at least one input file or folder".into());
    }
    if options.max_width == 0 {
        return Err("--max-width must be at least 1".into());
    }
    Ok(Parsed::Run(Args {
        inputs,
        output,
        options,
        json,
    }))
}

fn packable(path: &Path) -> bool {
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
fn collect(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
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

/// The sprites in one file. Notes about what could not be drawn exactly go
/// to stderr.
fn load(path: &Path) -> Result<Vec<Sprite>, String> {
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
        for note in &file.notes {
            eprintln!("oxypack: {}: {note}", path.display());
        }
        return Ok(sprites_from_aseprite(&file, &stem));
    }
    let is_doc = ext == "oxypix";
    let doc = if is_doc {
        load_oxypix(path)
    } else {
        import_image(path)
    }
    .map_err(|err| format!("{}: {err}", path.display()))?;
    sprites_from_document(&doc, &stem).map_err(|err| format!("{}: {err}", path.display()))
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

fn run(args: &Args) -> Result<String, String> {
    let files = collect(&args.inputs)?;
    let mut sprites = Vec::new();
    for file in &files {
        sprites.extend(load(file)?);
    }
    unique_names(&mut sprites);
    let atlas = pack_sprites(&sprites, args.options).map_err(|err| err.to_string())?;
    let png = atlas_png_bytes(&atlas).map_err(|err| err.to_string())?;
    let image = args
        .output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::fs::write(&args.output, png).map_err(|err| format!("{}: {err}", args.output.display()))?;
    if args.json {
        let path = args.output.with_extension("json");
        std::fs::write(&path, atlas_json(&atlas, &image))
            .map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(format!(
        "packed {} sprites from {} files into {}×{} {}",
        atlas.placements.len(),
        files.len(),
        atlas.width,
        atlas.height,
        args.output.display()
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Parsed::Help) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Parsed::Version) => {
            println!("oxypack {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Parsed::Run(args)) => match run(&args) {
            Ok(summary) => {
                println!("{summary}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("oxypack: {message}");
                ExitCode::from(1)
            }
        },
        Err(message) => {
            eprintln!("oxypack: {message}");
            eprintln!("Run `oxypack --help` for usage.");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn options_parse_in_any_order() {
        let Ok(Parsed::Run(args)) = parse(&strings(&[
            "--trim",
            "a.png",
            "-o",
            "out.png",
            "--padding",
            "0",
            "sprites",
            "--pot",
            "--dedupe",
            "--max-width",
            "512",
            "--no-json",
            "--packer",
            "maxrects",
        ])) else {
            panic!("expected a run");
        };
        assert_eq!(
            args.inputs,
            [PathBuf::from("a.png"), PathBuf::from("sprites")]
        );
        assert_eq!(args.output, PathBuf::from("out.png"));
        assert_eq!(
            args.options,
            PackOptions {
                packer: Packer::MaxRects,
                padding: 0,
                trim: true,
                dedupe: true,
                max_width: 512,
                power_of_two: true,
            }
        );
        assert!(!args.json);
    }

    #[test]
    fn missing_or_bad_arguments_are_usage_errors() {
        assert!(parse(&strings(&["a.png"])).is_err());
        assert!(parse(&strings(&["-o", "out.png"])).is_err());
        assert!(parse(&strings(&["a.png", "-o"])).is_err());
        assert!(parse(&strings(&["a.png", "-o", "x.png", "--padding", "lots"])).is_err());
        assert!(parse(&strings(&["a.png", "-o", "x.png", "--max-width", "0"])).is_err());
        assert!(parse(&strings(&["a.png", "-o", "x.png", "--rotate"])).is_err());
        assert!(parse(&strings(&[
            "a.png",
            "-o",
            "x.png",
            "--packer",
            "guillotine"
        ]))
        .is_err());
        assert!(parse(&strings(&["a.png", "-o", "x.png", "--packer"])).is_err());
        assert!(matches!(parse(&strings(&["--help"])), Ok(Parsed::Help)));
        assert!(matches!(parse(&strings(&["-V"])), Ok(Parsed::Version)));
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
