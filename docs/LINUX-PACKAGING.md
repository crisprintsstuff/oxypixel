# Linux packaging proposal

OxyPixel does **not** ship a Linux package in 0.1.0. This document is the plan for a later port. It is not a claim that a `.deb`, RPM, AppImage, or Flatpak exists or has been installed.

## What was checked on Ubuntu 24.04

On the machine that produced 0.1.0 (Ubuntu 24.04 under WSL2):

- `cargo test --workspace` passed (core, shortcuts, and the headless CLI).
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo build --release -p oxypixel` produced an ELF binary, and `oxypixel --help` ran.
- The window was not opened. That session had no `DISPLAY`, so interactive Linux GUI use is unverified.

That is a tested source build of the CLI and the library, not a supported desktop package.

Interactive use needs a display and a driver that can load `libGL.so.1`. The binary dlopens `libX11.so.6` and `libxkbcommon.so.0` through winit. Those are runtime libraries, not libraries this repository vendors.

## Proposed runtime dependencies

- `libgl1`
- `libx11-6`
- `libxkbcommon0`
- `libxcb1`

No GTK package is required. File paths are typed into the window on non-Windows builds.

## Proposed `.deb`

1. `cargo build --release -p oxypixel`
2. Install the binary to `/usr/bin/oxypixel`
3. Install `docs/linux/oxypixel.desktop` to `/usr/share/applications/`
4. Install a 32×32 PNG icon (exported from `installer/oxypixel.ico`) to `/usr/share/icons/hicolor/32x32/apps/oxypixel.png`
5. Add a shared-mime-info XML for the suffix `.oxypix` and the type `application/x-oxypix`
6. Declare the libraries above as `Depends`

`cargo-deb` or `nfpm` can pack that layout. Neither is configured in this repository yet.

## Proposed AppImage

Package the release binary with `linuxdeploy` and the desktop file above. Bundle `libxkbcommon` only if the target distro is older than the build machine. Do not bundle `libGL`.

## Proposed Flatpak

- App id: `com.oxypixel.OxyPixel` (not registered; pick this only if it is still free)
- Runtime: `org.freedesktop.Platform` 24.08
- Finish args: `--share=ipc`, `--socket=fallback-x11`, `--socket=wayland`, `--device=dri`, `--filesystem=home` (home is required so people can open documents)
- Build: a Cargo build of `-p oxypixel --release` inside the SDK, with the `x11` feature

Wayland is not enabled in 0.1.0. A Flatpak would still want the X11 socket, plus Wayland later if the egui `wayland` feature is turned on and tested.

## What has to be true before calling Linux supported

- `cargo test --workspace` and `cargo build --release -p oxypixel` pass on the distro you name
- Someone starts the window, draws a stroke, undoes it, saves a `.oxypix`, and exports a PNG on that distro
- The package installs and removes cleanly
- The README states the distro and the package format that were actually tested

Until those are done, point Linux users at the source build instructions in the README and at this document.
