# Architecture

OxyPixel is two crates.

| Crate | Role |
| --- | --- |
| `oxypixel-core` | Document, tools, history, import/export. No GUI. |
| `oxypixel` | egui window, shortcuts, and the small CLI. |

## Document

A document has a size, a palette, and a layer stack. Layer name, visibility, and opacity are shared by every frame. Each frame stores one cel (a straight-alpha RGBA buffer) per layer, plus a duration in milliseconds.

Compositing walks layers from bottom to top with source-over. Hidden layers and layers at opacity 0 are skipped. Pencil and eraser **replace** pixels on the active cel. They do not blend into the cel. Blend happens only when compositing.

Undo stores a clone of the document and the selection rectangle, capped at 50 steps and 256 MiB of pixel data. Tool choice, zoom, and mirror toggles stay session state. Slider edits (opacity, duration, rename) are coalesced while the control is active so one drag is one undo step.

## Tools

Geometry (line, rectangle, ellipse, flood fill, brush stamps) is pure and tested in `draw.rs`. The editor clamps pointer coordinates to a margin around the canvas before generating a shape, so a drag from `i32::MIN` cannot allocate a huge point list.

Shape tools preview until pointer release, then commit one undo step. Pencil commits during the drag as a single undo step. A selection, when present, clips stamps and flood fill. Dragging inside the selection moves those pixels on the active cel, and arrow keys nudge them. Copy stores the selection in an in-memory patch and publishes it as a straight-alpha image on the system clipboard. Paste uses that image when the clipboard has one. When the clipboard is missing, holds no image, or does not answer in time, paste uses the in-memory patch. Image conversion lives in `oxypixel-core`. The window publishes and reads the system clipboard, and it keeps the patch when that call fails. No selection means the whole canvas is editable. Mirror stamps the same point across the canvas center.

Flip and rotate use the selection, or every pixel of the active cel when nothing is selected. Each command is one undo step. Other layers and frames are unchanged, and the canvas size stays the same. Flips and a 180 degree turn keep the rectangle. A 90 degree turn swaps width and height and keeps the same center (integer division leaves an odd leftover toward the top and left). If the turned rectangle fits and still touches the cel, it shifts fully onto the cel. If it is wider or taller than the cel, pixels that land outside are dropped and the selection shrinks to the part that remains. A turn that misses the cel is refused and does not record an undo step.

## Files

`.oxypix` is a little-endian versioned blob. Version 1 is described in [FORMAT.md](FORMAT.md). Raster IO uses the `image` crate. GIF import uses that crate's composited frames. GIF export loops forever. Palette files are text, parsed in `io/palette.rs`: GIMP `.gpl`, a hex list (`.hex`), and JASC `.pal`. Import replaces the document palette and is one undo step.

## GUI

The window is egui 0.31 with the `glow` OpenGL backend. The canvas is a nearest-neighbor texture rebuilt when the document changes, and on every frame while a shape preview is active. The selection marquee is a dark stroke just outside the rectangle and an amber stroke just inside it. The status line prints the selection width and height while a selection exists. The frame strip shows a thumbnail of each composite. Onion skin ghosts earlier and later frames, without wrapping past either end of the animation. A preview panel beside the canvas shows the whole composite at an integer scale, can play the frame durations without changing the edited frame, and pans the canvas when you click it. Whether that panel is open is stored in the same data directory as recent files. Composites are cached: a stroke rebuilds the active frame, and playback reuses a frame that has not changed. File dialogs use `rfd`: the system dialog on Windows, and the XDG desktop portal on Linux. The path field stays available when the portal does not return a file. The Linux build does not need GTK development packages. Tool icons are drawn in code at 16×16.

A dirty document is also copied into the app data directory on a background thread, as a recovery `.oxypix` beside `recent.txt`. That copy is not the user's file. The next launch can restore or discard it. Export commands do not write it.

## Windows and Linux

The Windows release links the MSVC CRT statically (`target-feature=+crt-static`) and uses the GUI subsystem. A resource compiler, when present, embeds an icon and `VERSIONINFO`. The installer is NSIS and installs per user.

Linux uses the same code with egui's X11 feature. Tagged releases ship a tar.gz and a `.deb`. See [LINUX-PACKAGING.md](LINUX-PACKAGING.md).
