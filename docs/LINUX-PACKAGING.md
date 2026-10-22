# Linux packaging

Release 0.4.0 ships `oxypixel-0.4.0-linux-x86_64.tar.gz` (the x86_64 executable, `LICENSE`, and a short run note) and `oxypixel-0.4.0-linux-x86_64.deb`, built by [scripts/package-deb.sh](../scripts/package-deb.sh). The `.deb` version is the workspace package version plus Debian revision 1 (`0.4.0-1`). RPM, AppImage, and Flatpak are still only a plan.

## Why cargo-deb

`cargo-deb` 3.8.0 packs the `.deb`. `nfpm` does not.

The release job already builds the binary in `scripts/package-linux.sh`. `cargo deb --no-build` packs that binary, so the editor is not compiled a second time. `$auto` records the libraries the dynamic linker needs (`libc6` and the rest of the `DT_NEEDED` set). `libGL`, `libX11`, `libxkbcommon`, and `libxcb` are loaded with `dlopen`, so `$auto` does not see them. Those four packages are listed next to `$auto` in `crates/oxypixel/Cargo.toml`.

`nfpm` would need a separate `dpkg-shlibdeps` step for the linked libraries, and the package version would live in a second file. `scripts/package-deb.sh` downloads the cargo-deb 3.8.0 `.deb`, checks its SHA-256, and runs the binary from the cache. cargo-deb is a build tool only. It is not linked into `oxypixel`, and it is not a crate dependency.

## Runtime dependencies

The window needs:

- `libgl1` (`libGL.so.1`)
- `libx11-6` (`libX11.so.6`)
- `libxkbcommon0` (`libxkbcommon.so.0`)
- `libxkbcommon-x11-0` (`libxkbcommon-x11.so.0`)
- `libxcb1` (`libxcb.so.1`)

`libxkbcommon-x11-0` is required as well as `libxkbcommon0`. Opening the installed binary on Ubuntu 24.04 without it panicked inside `xkbcommon-dl` (`Library libxkbcommon-x11.so could not be loaded`). That package depends on `libxkbcommon0` and `libxcb-xkb1`.

No GTK package is required. Browse asks the XDG desktop portal for a file dialog. If no portal is running, type the path in the window.

The `.deb` also depends on the libraries the binary links. On Ubuntu 24.04 that includes `libc6`.

## `.deb` layout

`scripts/package-deb.sh` writes `dist/oxypixel-<version>-linux-x86_64.deb`.

| Path | Source |
| --- | --- |
| `/usr/bin/oxypixel` | `target/release/oxypixel` |
| `/usr/share/applications/oxypixel.desktop` | `docs/linux/oxypixel.desktop` |
| `/usr/share/icons/hicolor/32x32/apps/oxypixel.png` | `docs/linux/oxypixel.png`, the 32×32 image from `installer/oxypixel.ico` |
| `/usr/share/mime/packages/oxypixel.xml` | `docs/linux/oxypixel-mime.xml` (`application/x-oxypix`, suffix `.oxypix`) |
| `/usr/share/doc/oxypixel/copyright` | `LICENSE` |

The Debian version is `<version>-1` (revision 1). The GitHub asset name matches `PackageKind::LinuxDeb`: `oxypixel-<version>-linux-x86_64.deb`.

Install and remove:

```bash
sudo apt install ./oxypixel-<version>-linux-x86_64.deb
sudo apt remove oxypixel
```

## Release workflow

`.github/workflows/release.yml` runs `scripts/package-deb.sh` after the tar.gz on the Linux job. The publish job uploads the `.deb` with the existing files. `SHA256SUMS.txt` hashes, in order:

1. `oxypixel-<version>-windows-x86_64.exe`
2. `OxyPixel-<version>-Setup.exe`
3. `oxypixel-<version>-linux-x86_64.tar.gz`
4. `oxypixel-<version>-linux-x86_64.deb`

Check for updates looks up a full asset name. The `.deb` line does not hide the tar.gz. An unpacked copy still downloads the tar.gz (`PackageKind::LinuxArchive`). A copy whose path is `/usr/bin/oxypixel` downloads the `.deb` (`PackageKind::LinuxDeb`) and does not replace that file itself.

## Proposed AppImage

Package the release binary with `linuxdeploy` and the desktop file above. Bundle `libxkbcommon` only if the target distro is older than the build machine. Do not bundle `libGL`.

## Proposed Flatpak

- App id: `com.oxypixel.OxyPixel` (not registered; pick this only if it is still free)
- Runtime: `org.freedesktop.Platform` 24.08
- Finish args: `--share=ipc`, `--socket=fallback-x11`, `--socket=wayland`, `--device=dri`, `--filesystem=home` (home is required so people can open documents)
- Build: a Cargo build of `-p oxypixel --release` inside the SDK, with the `x11` feature

Wayland is not enabled. A Flatpak would still want the X11 socket, plus Wayland later if the egui `wayland` feature is turned on and tested.

## What has to be true before calling Linux supported

- `cargo test --workspace` and `cargo build --release -p oxypixel` pass on the distro you name
- Someone starts the window, draws a stroke, undoes it, saves a `.oxypix`, and exports a PNG on that distro
- The package installs and removes cleanly
- The README states the distro and the package format that were actually tested

The `.deb` from this tree was built on Ubuntu 24.04. `apt install` and `apt remove` were checked. `/usr/bin/oxypixel --help` ran, and `export-png` of a missing file printed an error and exited 1. With `libxkbcommon-x11-0` installed, the editor window opened as "OxyPixel - Untitled" and was then closed. A stroke, undo, save, and export from that window were not done, so this is not yet a full "Linux supported" check. Release 0.4.0 publishes that `.deb` next to the tar.gz. Release 0.3.2 published the tar.gz only.
