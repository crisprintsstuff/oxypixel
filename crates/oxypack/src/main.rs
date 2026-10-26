//! The `oxypack` command. Packing is in the library; this file reads the
//! command line, prints notes, and writes the two files. It never opens a
//! window or a dialog, so it is safe in build scripts and CI.

use std::path::PathBuf;
use std::process::ExitCode;

use oxypack::{pack_files, write_atlas, PackOptions, Packer};

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

fn run(args: &Args) -> Result<String, String> {
    let packed = pack_files(&args.inputs, args.options)?;
    for note in &packed.notes {
        eprintln!("oxypack: {note}");
    }
    write_atlas(&packed, &args.output, args.json)?;
    Ok(format!(
        "packed {} sprites from {} files into {}×{} {}",
        packed.atlas.placements.len(),
        packed.file_count,
        packed.atlas.width,
        packed.atlas.height,
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
}
