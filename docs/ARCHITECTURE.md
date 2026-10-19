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

Shape tools preview until pointer release, then commit one undo step. Pencil commits during the drag as a single undo step. A selection, when present, clips stamps and flood fill. Dragging inside the selection moves those pixels on the active cel, and arrow keys nudge them. Copy and paste use an in-memory clipboard, not the system clipboard. No selection means the whole canvas is editable. Mirror stamps the same point across the canvas center.

## Files

`.oxypix` is a little-endian versioned blob. Version 1 is described in [FORMAT.md](FORMAT.md). Raster IO uses the `image` crate. GIF import uses that crate's composited frames. GIF export loops forever.

## GUI

The window is egui 0.31 with the `glow` OpenGL backend. The canvas is a nearest-neighbor texture rebuilt when the document changes, and on every frame while a shape preview is active. File dialogs use `rfd`: the system dialog on Windows, and the XDG desktop portal on Linux. The path field stays available when the portal does not return a file. The Linux build does not need GTK development packages. Tool icons are drawn in code at 16×16.

## Windows and Linux

The Windows release links the MSVC CRT statically (`target-feature=+crt-static`) and uses the GUI subsystem. A resource compiler, when present, embeds an icon and `VERSIONINFO`. The installer is NSIS and installs per user.

Linux uses the same code with egui's X11 feature. Packaging is not produced. See [LINUX-PACKAGING.md](LINUX-PACKAGING.md).
