# OxyPixel

OxyPixel is an original pixel-art editor. It is not affiliated with Pixelorama, Orama Interactive, Godot, or Aseprite, and it does not include their code, names, or artwork.

The editor is a small Rust program. Drawing, layers, frames, undo, and file IO live in `oxypixel-core`, which has no windowing code. The `oxypixel` binary is an [egui](https://github.com/emilk/egui) interface on top of that library.

## Features in 0.1.0

- Pixel canvas with nearest-neighbor zoom, pan, and an optional grid
- Pencil, eraser, fill, eyedropper, line, rectangle, ellipse, and rectangular selection
- Primary and secondary colors, hex entry, and the built-in Ink palette
- Layers with visibility, opacity, rename, reorder, duplicate, and merge down
- Undo and redo
- Animation frames, playback, onion skin, GIF export, and a horizontal spritesheet
- Open and save `.oxypix` documents
- Import PNG, BMP, and GIF. Export PNG, BMP, GIF, and a PNG spritesheet
- Keyboard shortcuts

Canvas limits are intentional: at most 1024 pixels on a side, 1,048,576 pixels in one cel, and 16,777,216 pixels across every frame and layer. See [docs/FORMAT.md](docs/FORMAT.md).

## Install on Windows

The supported desktop package is the Windows installer.

1. Download `oxypixel.exe` and `OxyPixel-0.1.0-Setup.exe` from a GitHub release of this private repository, or build them with [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md).
2. Run `OxyPixel-0.1.0-Setup.exe`. It installs for the current user under `%LOCALAPPDATA%\OxyPixel`, adds a Start menu shortcut, and registers the `.oxypix` extension.
3. Launch OxyPixel from the Start menu.

**This build is unsigned.** No Authenticode certificate was available. Windows SmartScreen can warn that the publisher is unknown. That warning is expected. The executable and the installer are ordinary PE files and can be signed later with `signtool`. The signing commands are in [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md). Do not bypass SmartScreen for a binary you did not build or receive from a source you trust.

The app needs a GPU driver that provides OpenGL (`opengl32.dll`, which ships with Windows). It does not need the Visual C++ redistributable: release builds link the CRT statically.

## Build

Install Rust 1.99.0 (the `rust-toolchain.toml` pin) and then:

```bash
cargo test --workspace
cargo run -p oxypixel
```

Headless export, used by the tests and by scripts:

```bash
oxypixel export-png sprite.oxypix frame.png
oxypixel export-gif sprite.oxypix animation.gif
oxypixel export-sheet sprite.oxypix sheet.png
oxypixel export-bmp sprite.oxypix frame.bmp
```

Windows cross-compilation and the installer are documented in [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md). Shortcuts are in [docs/SHORTCUTS.md](docs/SHORTCUTS.md). The document model is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Linux

The same source builds on Linux because the editor is Rust and the Linux CI job compiles and tests it. That is not a packaged Linux release. There is no `.deb`, AppImage, or Flatpak in this version. The proposed packaging is in [docs/LINUX-PACKAGING.md](docs/LINUX-PACKAGING.md). Do not treat Linux as a supported desktop download unless that document says a package was built and tested.

A machine without an X11 display can still run `cargo test` and the export commands. Opening the window needs OpenGL and an X11 display (`libX11.so.6`, `libGL.so.1`).

## Assumptions

- The project lives in a new `oxylabs/oxypixel` directory.
- Windows is the packaged target. The GUI uses egui's `glow` backend.
- Animation is in the 0.1.0 core: one shared layer stack, per-frame cels, playback, GIF, and spritesheet.
- There is no code-signing certificate in this environment.
- The private GitHub repository, when created from this checkout, is `crisprintsstuff/oxypixel`.

## License

MIT. See [LICENSE](LICENSE). Direct dependency licenses are summarized in [docs/THIRD-PARTY.md](docs/THIRD-PARTY.md).
