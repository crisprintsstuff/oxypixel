# OxyPixel

OxyPixel is a pixel-art editor for making sprites and short animations on Windows and Linux. You draw on a pixel canvas, stack layers, and step through frames, then save an `.oxypix` document or export PNG, BMP, or GIF. The editing rules live in the `oxypixel-core` library, and the window is an [egui](https://github.com/emilk/egui) app. OxyPixel is an original program. It is not affiliated with Pixelorama, Orama Interactive, Godot, or Aseprite.

**[Releases](https://github.com/crisprintsstuff/oxypixel/releases)** · **[Changelog](CHANGELOG.md)** · **[v0.3.2](https://github.com/crisprintsstuff/oxypixel/releases/tag/v0.3.2)** · **[Roadmap](docs/ROADMAP.md)**

![The OxyPixel window editing a 64 by 64 drawing. Tools are on the left, the Ink palette and the Night and Cottage layers are on the right, and the frame strip is along the bottom.](docs/images/editor.png)

![The same drawing on frame 2, with onion skin showing the previous frame faintly. The cottage window is light and the smoke has moved.](docs/images/editor-frames.png)

The pictures above are screenshots of OxyPixel 0.3.2, taken from the running editor. The drawing is a sample file opened for the capture. The images are in [docs/images/](docs/images/) and are covered by the repository MIT license.

## Contents

- [Status](#status)
- [Download](#download)
- [Windows installer](#windows-installer)
- [Portable Windows build](#portable-windows-build)
- [Linux](#linux)
- [Features](#features)
- [How it's built](#how-its-built)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Build from source](#build-from-source)
- [File formats](#file-formats)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

## Status

Version 0.3.2 is an early release. The Windows downloads are unsigned. The Linux file on the v0.3.2 release is a tar.gz. Later tags also upload a `.deb`. A canvas side is at most 1024 pixels. What works today, and the open task list, is in [docs/ROADMAP.md](docs/ROADMAP.md).

## Download

Current release: [v0.3.2](https://github.com/crisprintsstuff/oxypixel/releases/tag/v0.3.2). Older builds are on the [releases page](https://github.com/crisprintsstuff/oxypixel/releases). What changed is in the [changelog](CHANGELOG.md).

| File | Use |
| --- | --- |
| [OxyPixel-0.3.2-Setup.exe](https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/OxyPixel-0.3.2-Setup.exe) | Windows installer |
| [oxypixel-0.3.2-windows-x86_64.exe](https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/oxypixel-0.3.2-windows-x86_64.exe) | Portable Windows executable |
| [oxypixel-0.3.2-linux-x86_64.tar.gz](https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/oxypixel-0.3.2-linux-x86_64.tar.gz) | Linux x86_64 archive |
| [SHA256SUMS.txt](https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/SHA256SUMS.txt) | Checksums |

The Windows files are unsigned. SmartScreen can warn that the publisher is unknown. Check `SHA256SUMS.txt` before you install. Signing steps are in [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md).

## Windows installer

`OxyPixel-0.3.2-Setup.exe` is a per-user NSIS setup.

1. Run the installer. It does not need administrator rights.
2. If SmartScreen says the publisher is unknown, use "More info" and then "Run anyway" after the checksum matches.
3. OxyPixel is installed under `%LOCALAPPDATA%\OxyPixel`.
4. The Start menu gets a shortcut, and `.oxypix` files open in OxyPixel. A desktop shortcut is optional.
5. Launch OxyPixel from the Start menu, or from the last page of setup.

The program needs OpenGL (`opengl32.dll`, which ships with Windows). The release executable links the C runtime statically, so the Visual C++ redistributable is not required.

## Portable Windows build

`oxypixel-0.3.2-windows-x86_64.exe` is the same 64-bit program without an installer. Put it in any folder and run it. SmartScreen can show the same unknown-publisher warning as the installer.

The v0.3.2 file on the releases page embeds the application icon and a version resource (`FileVersion` 0.3.2). A cross-build made without `rc.exe`, `llvm-rc`, or `windres` on `PATH` does not embed those resources, and File Explorer then shows a generic icon for the exe. The installer shortcut and the `.oxypix` association use `oxypixel.ico` either way. See [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md).

Help, then Check for updates, can download a newer release and restart into it after the checksum matches.

## Linux

Release 0.3.2 publishes `oxypixel-0.3.2-linux-x86_64.tar.gz`: the x86_64 executable, `LICENSE`, and `README.txt`, built on Ubuntu 24.04. Unpack it and run the program from that directory:

```bash
tar -xzf oxypixel-0.3.2-linux-x86_64.tar.gz
cd oxypixel-0.3.2-linux-x86_64
./oxypixel
```

Tagged releases after 0.3.2 also upload `oxypixel-<version>-linux-x86_64.deb`. That package installs `/usr/bin/oxypixel`, a desktop entry, a 32×32 icon, and the `.oxypix` mime type. On Ubuntu 24.04 the package from this tree was installed and removed with:

```bash
sudo apt install ./oxypixel-<version>-linux-x86_64.deb
sudo apt remove oxypixel
```

The v0.3.2 release on GitHub does not include that `.deb`. The download table above is the file that release published.

The window needs an X11 display and these libraries: `libGL.so.1`, `libX11.so.6`, `libxkbcommon.so.0`, and `libxkbcommon-x11.so.0`. Wayland copy and paste also loads `libwayland-client.so.0` when the compositor offers the data-control clipboard. The `.deb` depends on the packages that provide them (`libgl1`, `libx11-6`, `libxkbcommon0`, `libxkbcommon-x11-0`, `libxcb1`). Export commands do not need a display:

```bash
oxypixel export-png drawing.oxypix frame.png
```

RPM, AppImage, and Flatpak are not built. Notes are in [docs/LINUX-PACKAGING.md](docs/LINUX-PACKAGING.md).

Help, then Check for updates, downloads the tar.gz for an unpacked copy and the `.deb` when the running program is `/usr/bin/oxypixel`. A packaged copy stays in place. Install the downloaded `.deb` with the package manager.

## Features

These match this tree. The latest release tag is 0.3.2. Planned work is listed in [docs/ROADMAP.md](docs/ROADMAP.md) and is not included here.

- Pixel canvas with nearest-neighbor zoom, pan, and an optional grid
- Pencil, eraser, fill, eyedropper, line, rectangle, ellipse, and rectangular selection
- Move and nudge a selection. Copy publishes a straight-alpha image to the system clipboard and keeps an in-memory patch. Paste uses the system image, or that patch when the clipboard has no image
- Canvas resize and crop
- Horizontal and vertical mirror while drawing
- Primary and secondary colors, hex entry, and the Ink palette
- Layers with visibility, opacity, rename, reorder, duplicate, and merge down
- Undo and redo, including the selection rectangle
- Animation frames, playback, onion skin, GIF export, and a horizontal spritesheet
- PNG and spritesheet export at 1×, 2×, or 4× nearest-neighbor
- `.oxypix` documents, plus PNG, BMP, and GIF import and export
- Recent files, drag-and-drop, a Windows file dialog, and a Linux portal dialog with a path field if the portal is unavailable
- Headless `export-png`, `export-bmp`, `export-gif`, and `export-sheet` commands
- Keyboard shortcuts

A canvas side is at most 1024 pixels. One cel holds at most 1,048,576 pixels. The whole document holds at most 16,777,216 pixels across frames and layers. Details are in [docs/FORMAT.md](docs/FORMAT.md).

## How it's built

The maintainer develops OxyPixel with AI coding agents. The written rules for that work are in [AGENTS.md](AGENTS.md): which crate owns which behavior, errors instead of panics on bad input, and no modal dialogs on headless, CLI, or test paths. Changes are expected to include tests. GitHub Actions runs rustfmt, clippy with warnings denied, and the test suite on Linux and Windows. The maintainer reviews and merges pull requests.

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| B, E, G, I | Pencil, eraser, fill, eyedropper |
| L, R, O, M | Line, rectangle, ellipse, select |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+C / Ctrl+V | Copy / paste the selection image |
| Arrow keys | Nudge the selection |
| Comma / Period | Previous / next frame |
| Ctrl+N / Ctrl+O / Ctrl+S | New, open, save |
| Enter | Play or pause |
| Space+drag | Pan |
| Ctrl+Plus / Ctrl+Minus / Ctrl+0 | Zoom in, zoom out, fit |

The full list is [docs/SHORTCUTS.md](docs/SHORTCUTS.md).

## Build from source

Rust 1.99.0 is pinned in `rust-toolchain.toml`.

```bash
cargo test --workspace
cargo run -p oxypixel
```

Headless export:

```bash
oxypixel export-png sprite.oxypix frame.png
oxypixel export-bmp sprite.oxypix frame.bmp
oxypixel export-gif sprite.oxypix animation.gif
oxypixel export-sheet sprite.oxypix sheet.png
```

| Script | Result |
| --- | --- |
| [scripts/build-windows.sh](scripts/build-windows.sh) | Portable `oxypixel-<version>-windows-x86_64.exe` |
| [scripts/build-installer.sh](scripts/build-installer.sh) | `OxyPixel-<version>-Setup.exe` |
| [scripts/package-linux.sh](scripts/package-linux.sh) | `oxypixel-<version>-linux-x86_64.tar.gz` |
| [scripts/package-deb.sh](scripts/package-deb.sh) | `oxypixel-<version>-linux-x86_64.deb` |

`make test` runs the test suite. `make check` runs rustfmt, clippy, and the tests.

## File formats

`.oxypix` is the editor's own little-endian document. Version 1 stores the canvas, shared layer stack, frame durations, pixels, and palette. The byte layout is [docs/FORMAT.md](docs/FORMAT.md).

PNG, BMP, and GIF can be imported as a new document or as a layer. A GIF import becomes one composited layer named Imported. Export writes the composited frame, a GIF animation, a BMP, or a horizontal PNG spritesheet.

## Documentation

| Guide | What it covers |
| --- | --- |
| [CHANGELOG.md](CHANGELOG.md) | Release history |
| [AGENTS.md](AGENTS.md) | Rules for contributors and coding agents |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Scorecard and the open task ids |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Core library and the egui window |
| [docs/SHORTCUTS.md](docs/SHORTCUTS.md) | Keyboard and mouse shortcuts |
| [docs/FORMAT.md](docs/FORMAT.md) | `.oxypix` layout and size limits |
| [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md) | Windows build, installer, and code signing |
| [docs/LINUX-PACKAGING.md](docs/LINUX-PACKAGING.md) | Linux tar.gz, `.deb`, and packages that are still only a plan |
| [docs/THIRD-PARTY.md](docs/THIRD-PARTY.md) | Direct dependency licenses |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Tests, pull requests, and version bumps |
| [SECURITY.md](SECURITY.md) | File parsing limits and how to report a problem |

## Contributing

Bug reports and small pull requests are welcome. Open a [GitHub issue](https://github.com/crisprintsstuff/oxypixel/issues) for a defect, and use the path in [SECURITY.md](SECURITY.md) for a security problem. Use a focused branch. Run `make check` before asking for review. Do not copy code or artwork from other pixel editors.

People and coding agents working in the tree should read [AGENTS.md](AGENTS.md) and take the next open id from [docs/ROADMAP.md](docs/ROADMAP.md). The full notes are in [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

OxyPixel does not open a network connection on its own. Help, then Check for updates, is the one request to GitHub, and it downloads a file only after `SHA256SUMS.txt` matches. Document parsing rejects oversized files before it allocates the pixel buffers. Report a problem the way [SECURITY.md](SECURITY.md) describes.

## License

MIT. See [LICENSE](LICENSE).
