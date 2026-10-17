//! Embed a Windows icon and VERSIONINFO when a resource compiler is on PATH.
//! The build still succeeds without one. The window icon is also set at runtime.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/oxypixel.rc");
    println!("cargo:rerun-if-changed=assets/oxypixel.ico");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let Some(compiler) = find_compiler() else {
        println!(
            "cargo:warning=No Windows resource compiler (llvm-rc, rc, or windres) on PATH. \
             oxypixel.exe will not embed an icon or VERSIONINFO. The installer still ships oxypixel.ico."
        );
        return;
    };
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let res = PathBuf::from(&out_dir).join("oxypixel.res");
    let rc = "assets/oxypixel.rc";
    let ok = compile(&compiler, rc, &res);
    if !ok {
        println!("cargo:warning=Windows resource compiler failed; continuing without embedded resources.");
        return;
    }
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

fn find_compiler() -> Option<String> {
    for name in ["llvm-rc", "rc", "x86_64-w64-mingw32-windres", "windres"] {
        if Command::new(name).arg("--version").output().is_ok()
            || Command::new(name).arg("/?").output().is_ok()
        {
            return Some(name.to_string());
        }
    }
    None
}

fn compile(compiler: &str, rc: &str, res: &std::path::Path) -> bool {
    let output = res.to_string_lossy().to_string();
    let result = if compiler.contains("windres") {
        Command::new(compiler)
            .args([
                "--input",
                rc,
                "--output-format",
                "coff",
                "--output",
                &output,
            ])
            .status()
    } else {
        Command::new(compiler).args(["/fo", &output, rc]).status()
    };
    result.map(|s| s.success()).unwrap_or(false)
}
