# Architecture

OxyPixel is two crates.

| Crate | Role |
| --- | --- |
| `oxypixel-core` | Document, tools, history, import/export. No GUI. |
| `oxypixel` | egui window, shortcuts, and the small CLI. |

## Document

A document has a size, a palette, and a layer stack. Layer name, visibility, and opacity are shared by every frame. Each frame stores one cel (a straight-alpha RGBA buffer) per layer, plus a duration in milliseconds.

Compositing walks layers from bottom to top. Each layer mixes with what is below it by its blend mode (`blend.rs`: the W3C compositing formulas plus Addition, Subtract, and Divide); Normal is plain source-over. Merge down bakes the upper layer's mode into the lower layer. Hidden layers and layers at opacity 0 are skipped. Pencil and eraser **replace** pixels on the active cel. They do not blend into the cel. Blend happens only when compositing.

Reference images are not layers. `References` holds up to eight decoded PNG or JPEG pictures, plus placement. `Document::composite`, merge down, and every exporter take a `Document` and do not read `References`. The eyedropper reads a reference only when that image's Sample flag is on. Copy onto layer is a separate step: it rasterizes one reference into a new layer, and that layer is then part of the document. The `.oxypix` version stays 1, which rejects trailing bytes, so paths and placement are a sidecar named `<file>.oxypix.refs` next to the document, written on Save. Paths inside the document's folder are relative to it. The pixels stay in the image file the path points at.

Undo stores a clone of the document and the selection rectangle, capped at 50 steps and 256 MiB of pixel data. Tool choice, zoom, and mirror toggles stay session state. Slider edits (opacity, duration, rename) are coalesced while the control is active so one drag is one undo step.

## Tools

Geometry (line, rectangle, ellipse, flood fill, brush stamps) is pure and tested in `draw.rs`. The editor clamps pointer coordinates to a margin around the canvas before generating a shape, so a drag from `i32::MIN` cannot allocate a huge point list.

Shape tools preview until pointer release, then commit one undo step. Pencil commits during the drag as a single undo step. Pixel-perfect, off until it is turned on, drops the extra corner of a one-pixel pencil step and still records one undo step. A brush larger than one pixel and the line tool ignore it. Wrap, also off until it is turned on, continues a stamp or a fill that leaves an edge on the opposite edge. The center mirrors stay reflections across the canvas. A tiled view draws that same cel 3 by 3; export still writes one copy. Reference overlays are drawn once on the real canvas and are not part of those copies. A selection is a bounding rectangle with an optional per-pixel mask (`selection.rs`), built from marquees and magic wand clicks that Shift adds and Alt cuts out. When present, it clips stamps and flood fill to the selected pixels. Dragging inside the selection moves those pixels on the active cel, and arrow keys nudge them. Copy stores the selection in an in-memory patch and publishes it as a straight-alpha image on the system clipboard. Paste uses that image when the clipboard has one. When the clipboard is missing, holds no image, or does not answer in time, paste uses the in-memory patch. Image conversion lives in `oxypixel-core`. The window publishes and reads the system clipboard, and it keeps the patch when that call fails. No selection means the whole canvas is editable. Mirror stamps the same point across the canvas center.

Flip and rotate use the selection, or every pixel of the active cel when nothing is selected. Each command is one undo step. Other layers and frames are unchanged, and the canvas size stays the same. Flips and a 180 degree turn keep the rectangle. A 90 degree turn swaps width and height and keeps the same center (integer division leaves an odd leftover toward the top and left). If the turned rectangle fits and still touches the cel, it shifts fully onto the cel. If it is wider or taller than the cel, pixels that land outside are dropped and the selection shrinks to the part that remains. A turn that misses the cel is refused and does not record an undo step.

## Files

`.oxypix` is a little-endian versioned blob, described in [FORMAT.md](FORMAT.md). Version 2 adds sections after the palette and is written only when a document uses one: animation tags and frame names (optional), and layer blend modes (required). Tags live on the `Document`, so undo snapshots carry them, and duplicating or deleting a frame moves or shrinks the tags around it. Raster IO uses the `image` crate. GIF import uses that crate's composited frames. GIF export loops forever. Palette files are text, parsed in `io/palette.rs`: GIMP `.gpl`, a hex list (`.hex`), and JASC `.pal`. Import replaces the document palette and is one undo step. Dragging a swatch records one undo step, and a ramp between two swatches does too. Snap, off until it is turned on, makes pencil, fill, and shapes stamp the nearest palette color. The eraser still clears. Export writes the swatches in their current order. Reference placement is the sidecar described above. It is not parsed as part of the document.

## Settings

App settings are a plain `key=value` file, `view.txt`, in the data directory beside the recent files list (`session.rs`). A missing file, an unknown key, or a value out of range keeps that setting's default, so an older or hand-edited file never stops the window from opening. The Settings window (`settingsview.rs`) edits a copy and reports the change; the app applies only what changed to the open view and saves the file at once. Autosave keeps its own flag file in the recovery directory and recent files their own list. The checkerboard behind clear pixels is one process-wide setting in `canvas.rs`; changing it drops the textures that have the old one baked in.

## MCP server

`oxypixel mcp` (`crates/oxypixel/src/mcp/`) is a Model Context Protocol server written from the public specification, without an SDK: newline-delimited JSON-RPC 2.0 on stdin and stdout, parsed with `serde_json`, which the editor already uses. `mod.rs` reads one capped line at a time, answers `initialize` (protocol versions 2025-06-18, 2025-03-26, and 2024-11-05), `ping`, `tools/list`, and `tools/call`, ignores notifications, and resolves paths against `--root`. `tools.rs` holds each tool's JSON schema and handler. A call loads the document, works through `Editor` (drawing goes through the same pointer calls as the canvas, so lines, shapes, and fills match the editor), and saves it; a failed call returns an error result and leaves the file alone. `render` composites a frame and returns a PNG as base64 image content. Packing calls the `oxypack` library. Nothing in the module opens a window, and the release build's GUI subsystem on Windows still gets the client's pipes.

## OxyPack

`crates/oxypack` is a library and a second binary on the same core. The editor's File, then Pack sprites… window (`packview.rs`) uses the library's `pack_files` and `write_atlas`, the same calls the command makes. It uses only the core's public API (loading `.oxypix`, importing images, compositing frames), so it could move to its own repository by copying the folder. Its `pack.rs` turns each frame of a document into a sprite, makes the names unique, trims and dedupes, places the sprites with a shelf packer (tallest first, rows no wider than the limit, input order breaking ties, so the same inputs give the same atlas) or, with `--packer maxrects`, MaxRects with the bottom-left rule in the same order, and writes the PNG and the JSON hash. `aseprite.rs` reads `.aseprite` files from the published specification: a bounds-checked reader, sizes checked before allocation (16384 a side, 4096 frames, all frames within one atlas, the 80 MiB file cap), zlib cels that must inflate to exactly their size, and per-frame compositing down the layer tree: each layer blends with its mode (the core's `blend`, the same one the editor uses), and a group is drawn into its own buffer first when the file says groups composite separately. Tilemap cels are drawn into an image from their layer's tileset once the frame's chunks are read, then composite like image cels. `files.rs` collects inputs, loads them, and writes the PNG and JSON. `main.rs` only parses arguments and prints notes and the summary. It has no GUI dependency and never opens a dialog. Atlas sizes use the spritesheet caps: 16384 on a side and 64 Mi pixels.

## GUI

The window is egui 0.31. It starts with the wgpu renderer (Direct3D 12 on Windows, including the WARP software adapter, and Vulkan or OpenGL through EGL on Linux). If wgpu cannot find an adapter or make a surface, `main` starts it again with the `glow` OpenGL renderer. The canvas is a nearest-neighbor texture rebuilt when the document changes, and on every frame while a shape preview is active. The selection marquee is a dark stroke just outside the rectangle and an amber stroke just inside it. The status line prints the selection width and height while a selection exists. The frame strip shows a thumbnail of each composite. Onion skin ghosts earlier and later frames, without wrapping past either end of the animation. A preview panel beside the canvas shows the whole composite at an integer scale, can play the frame durations without changing the edited frame, and pans the canvas when you click it. Tiled view repeats the canvas 3 by 3; the preview stays one composite. Whether that panel is open is stored in the same data directory as recent files. Composites are cached: a stroke rebuilds the active frame, and playback reuses a frame that has not changed. File dialogs use `rfd`: the system dialog on Windows, and the XDG desktop portal on Linux. The path field stays available when the portal does not return a file. The Linux build does not need GTK development packages. Tool icons are drawn in code at 16×16.

A dirty document is also copied into the app data directory on a background thread, as a recovery `.oxypix` beside `recent.txt`, with its reference list in a `.oxypix.refs` next to it. That copy is not the user's file. The next launch can restore or discard it. Export commands do not write it.

## Windows and Linux

The Windows release links the MSVC CRT statically (`target-feature=+crt-static`) and uses the GUI subsystem. A resource compiler, when present, embeds an icon and `VERSIONINFO`. The installer is NSIS and installs per user.

Linux uses the same code with egui's X11 feature. Tagged releases ship a tar.gz and a `.deb`. See [LINUX-PACKAGING.md](LINUX-PACKAGING.md).
