# Third-party notices

OxyPixel is MIT licensed. The crates below are direct dependencies. Their own transitive dependencies stay under the licenses those crates declare. Run `cargo metadata --format-version 1` for the full resolved tree.

| Crate | Use | License |
| --- | --- | --- |
| eframe / egui / epaint | Window, widgets, painting | MIT OR Apache-2.0 |
| image, png, gif | PNG, JPEG, BMP, and GIF. JPEG is the `image` crate's jpeg feature, used for reference images | MIT OR Apache-2.0 |
| thiserror | Error messages | MIT OR Apache-2.0 |
| rfd (Windows only) | Open and save dialogs | MIT |
| arboard | System clipboard images on Windows and Linux | MIT OR Apache-2.0 |

The Ink palette, the application icon, and the `.oxypix` layout were made for this project.

NSIS, used only to build the installer, is under the zlib/libpng license and is not linked into `oxypixel.exe`.

cargo-deb 3.8.0, used only to build the Linux `.deb`, is MIT licensed and is not linked into `oxypixel`. `scripts/package-deb.sh` downloads that release and checks its SHA-256.
