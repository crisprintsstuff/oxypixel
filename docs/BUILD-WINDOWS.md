# Windows build and installer

The packaged artifacts are a 64-bit `oxypixel-0.2.0-windows-x86_64.exe` plus `OxyPixel-0.2.0-Setup.exe`. Both are **unsigned** unless you add a certificate yourself. They are normal PE files, so `signtool` can sign them later. This repository does not contain a certificate and the build scripts never look for one.

## What the executable links

Release builds pass `-C target-feature=+crt-static` for `x86_64-pc-windows-msvc` (see `.cargo/config.toml`). The CRT is inside the exe. The process still loads system DLLs, including `opengl32.dll`.

`crates/oxypixel/assets/oxypixel.rc` is the VERSIONINFO and icon script. `build.rs` embeds it when `llvm-rc`, `rc`, or `windres` is on `PATH`. If none of those exist, the build continues and prints a cargo warning. The installer still copies `oxypixel.ico` and uses it for the shortcut and the `.oxypix` icon.

The cross-compiled `dist/oxypixel.exe` from a Linux machine without those tools has no embedded icon or VERSIONINFO. File Explorer shows a generic icon for the exe itself. The Start menu shortcut still uses `oxypixel.ico`. The GitHub Actions Windows job adds `rc.exe` to `PATH` when the Windows SDK provides it, so the tagged release build can embed the resource.

## Build on Windows

From a Visual Studio x64 prompt, or any shell where `cargo` can see the MSVC linker:

```bat
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
rustup target add x86_64-pc-windows-msvc
cargo test --workspace
cargo build --release -p oxypixel
```

The executable is `target\release\oxypixel.exe`.

Install [NSIS 3](https://nsis.sourceforge.io/) so `makensis` is on `PATH`, then:

```bat
if not exist dist mkdir dist
copy /Y target\release\oxypixel.exe dist\oxypixel.exe
copy /Y installer\oxypixel.ico dist\oxypixel.ico
makensis installer\oxypixel.nsi
```

The installer is `dist\OxyPixel-0.2.0-Setup.exe`. It is a per-user install (`RequestExecutionLevel user`) into `%LOCALAPPDATA%\OxyPixel`, with an uninstaller, Start menu entries, an optional desktop shortcut, and an HKCU `.oxypix` association. Add/Remove Programs reads the uninstall key under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\OxyPixel`.

## Cross-compile from Linux

This is how a Linux machine produces the same MSVC executable. It needs:

- Rust 1.99.0 with the `x86_64-pc-windows-msvc` target
- [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin) (`cargo install cargo-xwin`)
- `clang-cl` on `PATH` (LLVM's clang, or the Swift toolchain's `clang-cl`)

```bash
./scripts/build-windows.sh
./scripts/build-installer.sh
```

`build-windows.sh` runs `cargo xwin build --release --target x86_64-pc-windows-msvc` and copies `dist/oxypixel.exe` and `dist/oxypixel.ico`. `build-installer.sh` runs NSIS inside an `ubuntu:24.04` container (`apt-get install nsis`) so the host does not need root. Docker has to be able to pull that image.

## Reproducibility

- `rust-toolchain.toml` pins 1.99.0
- `Cargo.lock` is committed
- The Windows target always gets `+crt-static`
- `scripts/build-windows.sh` copies `dist/oxypixel-<version>-windows-x86_64.exe`

A GitHub Actions run on `windows-latest` (`.github/workflows/release.yml`) is the native build path. The Linux cross build is a convenience for machines that are not Windows. Prefer the Actions artifact when the two ever disagree.

## Code signing

No signature is applied. After you have a certificate, sign the application and then the installer, from a Windows machine:

```bat
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f your-cert.pfx /p YOUR_PASSWORD dist\oxypixel.exe
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f your-cert.pfx /p YOUR_PASSWORD dist\OxyPixel-0.2.0-Setup.exe
signtool verify /pa /v dist\oxypixel.exe
signtool verify /pa /v dist\OxyPixel-0.2.0-Setup.exe
```

Do not put the `.pfx` or the password in the repository, in CI logs, or in a script that gets committed. A GitHub Actions secret named something like `WINDOWS_CERT_PFX` (base64) and `WINDOWS_CERT_PASSWORD` can feed `signtool` later. Until that exists, tell users the build is unsigned.

SmartScreen will warn on unsigned software. That is a property of this build, not a defect in the installer script.
