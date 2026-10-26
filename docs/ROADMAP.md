# Roadmap and scorecard

Status date: 2026-10-26. Version in tree: 0.7.5 (`Cargo.toml`). GitHub issues: none open. This file is the work list for [AGENTS.md](../AGENTS.md).

Rows are taken from the code and the docs. **Proposal** means the item is a reasonable next step, not a commitment and not an open issue. Do not start a proposal unless a human asks for it.

When you finish a task, change its status here in the same pull request.

## Current focus

Infrastructure, then small fixes that already have a place in the code.

OX-002, OX-011, and OX-012 are done, so this list is empty.

Suggested order if a human picks proposals: OX-P20, OX-P21, OX-P6, OX-P22, OX-P24, OX-P25, and OX-P23 are done. OX-P27 waits for a format decision.

## Scorecard

### Done

In the 0.7.5 tree and covered by tests or by the release workflow.

| Area | What exists | Where |
| --- | --- | --- |
| Tools | Pencil, eraser, fill, eyedropper, line, rectangle, ellipse, rectangular select. Brush 1–16. Filled or outline shapes. Optional one-pixel pixel-perfect pencil. Fill tolerance 0–255 and a global fill. Shading along the palette (D) | `editor.rs`, `draw.rs` |
| Selection | Rectangle and magic wand, Shift to add and Alt to cut out, so a selection can be any shape. Drag to move, arrow nudge, flip, rotate, system-clipboard and in-memory copy and paste, clear, select all. The status line shows the selection size | `editor.rs`, `selection.rs`, `clipboard.rs`, `app.rs`, `canvas.rs` |
| Canvas | Resize and crop. Mirror horizontal and vertical while stamping. Optional wrap, and a 3×3 tiled view that export ignores | `document.rs`, `editor.rs`, `canvas.rs` |
| Layers | Add, delete, duplicate, reorder, rename, visibility, opacity, merge down. At most 32 | `document.rs` |
| Frames | Add, duplicate, duration 20–10000 ms on the slider, playback clock, thumbnails, onion skin for earlier and later frames | `animation.rs`, `canvas.rs`, `framestrip.rs` |
| Undo | Up to 50 steps and 256 MiB, including the selection rectangle. Slider drags coalesce | `history.rs` |
| `.oxypix` | Version 1, little-endian, caps before allocation, trailing bytes rejected | `io/oxypix.rs`, `docs/FORMAT.md` |
| Raster | PNG, BMP, and GIF import and export. GIF import is one composited layer. Spritesheet in a row or a grid with padding and a JSON description, and 2×/4× nearest PNG | `io/raster.rs` |
| References | PNG and JPEG overlays or a floating panel. Placement is a sidecar, not a `.oxypix` field. Export, merge, and the eyedropper skip them unless Sample is on | `reference.rs`, `refsview.rs` |
| OxyPack | `oxypack` library and command, and File, then Pack sprites… in the editor: PNG, BMP, GIF, `.oxypix`, and `.aseprite` inputs or folders, a shelf- or MaxRects-packed atlas PNG and a JSON hash, padding, width limit, trim, dedupe, power-of-two sizes | `crates/oxypack/` |
| CLI | `export-png`, `export-bmp`, `export-gif`, `export-sheet` (with `--columns`, `--padding`, `--scale`, `--json`), `export-palette`, and `import-palette`. Failures print and exit. Missing file must not hang | `cli.rs`, `tests/cli.rs` |
| Palette files | GIMP `.gpl`, hex `.hex` (one `RRGGBB` per line, `RRGGBBAA` when alpha is not 255), and JASC `.pal`. Import replaces the palette and is one undo step. Swatches drag to a new index. A ramp of N steps can fill the span between two swatches. Snap (S) makes pencil, fill, and shapes use the nearest palette color. More than 256 colors is an error | `io/palette.rs`, `editor.rs`, `color.rs`, `app.rs` |
| Window | egui 0.31 on wgpu (Direct3D 12 with WARP on Windows, Vulkan on Linux), falling back to glow OpenGL, nearest-neighbor canvas, whole-pixel zoom, a grid when the cells are readable, pan, shortcuts, tool icons with their shortcut letter, a preview panel beside the canvas | `app.rs`, `canvas.rs`, `preview.rs` |
| Files UI | Recent list of ten paths, a folder hint when names match, a missing-file mark, and Clear recent. Drag and drop, Windows file dialog, Linux portal with a path field fallback | `recent.rs`, `native.rs` |
| Recovery | Autosave writes a `.oxypix` in the data directory about once a minute and after 30 edits, with the reference list beside it. Startup can restore or discard it. A save, a discard, or a clean close removes it | `recovery.rs`, `app.rs` |
| Updates | Help, then Check for updates. Download only after `SHA256SUMS.txt` matches. No network until that command | `update.rs` |
| Windows package | Per-user NSIS installer and portable exe. Static CRT. Unsigned | `installer/oxypixel.nsi`, `release.yml` |
| Portable exe resources | The tagged Windows build embeds an icon and VERSIONINFO from `oxypixel.rc` when `rc.exe` is on `PATH`. A cross-build without `rc.exe`, `llvm-rc`, or `windres` does not | `build.rs`, [README.md](../README.md), [BUILD-WINDOWS.md](BUILD-WINDOWS.md) |
| Linux package | `oxypixel-<version>-linux-x86_64.tar.gz` and `oxypixel-<version>-linux-x86_64.deb` | `scripts/package-linux.sh`, `scripts/package-deb.sh`, `release.yml` |
| CI | One Windows job per pull request. `push` is `main` only; tags stay on `release.yml`. Docs-only changes skip the matrix. Ubuntu also runs `make check-windows`. `ubuntu-latest` and `windows-latest`, fmt, clippy `-D warnings`, tests, release build. Windows then launches the release exe and closes it. Job timeout 15 minutes | `.github/workflows/ci.yml`, `Makefile` |

### Partial

| Area | What is true now | Gap |
| --- | --- | --- |
| Clipboard | Copy publishes a straight-alpha image and keeps the in-memory patch. Paste uses a system image when one is present | A Wayland session with neither the data-control protocol nor X11 has no system clipboard. On Linux the image is served while OxyPixel is open. See [OX-P1](#ox-p1) |
| GIF import | Every frame becomes a cel on one layer named Imported | Source layers are not in the GIF. This matches [docs/FORMAT.md](FORMAT.md) |
| File dialogs | Windows system dialog. Linux XDG portal, then the path field | No macOS build |
| Linux packages | The release archive and a `.deb` (desktop entry, 32×32 icon, `.oxypix` mime type) are built and uploaded. See [OX-P2](#ox-p2) | RPM, AppImage, and Flatpak are only described in [LINUX-PACKAGING.md](LINUX-PACKAGING.md) |
| Code signing | `docs/BUILD-WINDOWS.md` tells a human how to run `signtool` | No certificate in the repo or in CI. See [OX-H1](#ox-h1) |
| GUI checks | Canvas math, shortcuts, the CLI, and a headless hash of the built-in example's composite against `crates/oxypixel-core/tests/golden/` | Nothing clicks the window. Refresh that golden with the command in [OX-P3](#ox-p3) |

### Missing

Nothing below is an open GitHub issue. The proposals are optional. The human-only row is not agent work.

## Tasks

### OX-001

**Stop the duplicate Windows CI run.** Status: done. Area: `.github/workflows/ci.yml`.

`push` runs only on `main`. Pull requests use `pull_request`, so a branch no longer starts a second workflow. A concurrency group cancels a run that a newer push on the same ref replaces. Tag pushes stay on `release.yml`. A change that only touches `*.md` or `docs/**` skips the Ubuntu and Windows jobs with a job condition, which reports success. `paths-ignore` was not used: a workflow skipped that way never reports, and a required check stays pending. The comment at the top of `ci.yml` says why. The pull request's own CI is the check. `release.yml` was not dispatched.

### OX-002

**One command for Windows-target clippy.** Status: done. Area: `Makefile`, `AGENTS.md`, `.github/workflows/ci.yml`.

`make check-windows` runs `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. The target comment names the MinGW compiler `x86_64-w64-mingw32-gcc`. If that compiler, `rustup`, or the `x86_64-pc-windows-gnu` target is missing, it prints the install command and exits 1. A clean tree exits 0. [AGENTS.md](../AGENTS.md) tells agents to run it when a diff touches Windows-only code. The Ubuntu CI job installs the compiler and runs the same target, so a Windows-only lint fails on the runner that is not billed double. The Windows job still runs host clippy.

### OX-010

**Reconcile the portable-exe icon sentence.** Status: done. Area: `README.md`, `docs/BUILD-WINDOWS.md`.

The v0.3.2 portable exe on the releases page (`oxypixel-0.3.2-windows-x86_64.exe`) contains an ICON, a GROUP_ICON, and a VERSION resource. `FileVersion` and `ProductVersion` are 0.3.2. README and `docs/BUILD-WINDOWS.md` now say the same thing: that tagged Windows build embeds the icon and version resource, and a cross-build without `rc.exe`, `llvm-rc`, or `windres` does not. No new release was cut.

### OX-011

**Lock update filenames to `PackageKind`.** Status: done. Area: `crates/oxypixel/src/update.rs`.

`release_checksum_list_names_each_package` builds the portable exe, installer, and Linux archive names from `Version::current()` and `PackageKind::asset_name`, then checks that `expected_hash` finds each of them. A `dist/` prefix is not a match. `deb_asset_does_not_replace_the_linux_archive` keeps a fixed v0.3.2 release payload, including `oxypixel-0.3.2-linux-x86_64.deb`, so that sample does not follow the package version.

### OX-012

**Test the recent-files directory.** Status: done. Area: `crates/oxypixel/src/recent.rs`.

`data_dir_uses_the_platform_folder` sets `APPDATA` on Windows and `XDG_DATA_HOME` elsewhere, calls `data_dir`, and checks that the last path component is `OxyPixel` or `oxypixel`. The variable is restored when the test returns. The test does not open a dialog or write a file.

## Proposals

Not scheduled. Start one only if a human asks.

### OX-P1

**System clipboard for the selection.** Status: done.

Copy stores the selection in the in-memory patch and publishes that patch as a straight-alpha image when the OS clipboard answers. Paste loads a system image into the patch and paints it on the active cel. A missing clipboard, a clipboard with no image, or a call that does not answer in time pastes the patch already in memory. A rejected image leaves the previous patch in place. `oxypixel-core` does not depend on a clipboard crate. The window uses `arboard`.

Windows writes a PNG clipboard format and a 32-bit DIB, so a program that reads PNG keeps transparency. Linux writes `image/png` on X11, and on Wayland when the compositor offers the data-control protocol. If that Wayland setup fails and `DISPLAY` is set, the X11 clipboard is used. No display and no usable Wayland clipboard does not block and does not panic. The Linux image stays available to other programs while OxyPixel is open; after quit it remains only if a clipboard manager saved it.

### OX-P2

**A `.deb` as written in LINUX-PACKAGING.md.** Status: done. Area: `scripts/package-deb.sh`, `.github/workflows/release.yml`, `crates/oxypixel/src/update.rs`.

`cargo-deb` 3.8.0 packs the release binary (`cargo deb --no-build`). Tagged releases upload `oxypixel-<version>-linux-x86_64.deb` beside the tar.gz and list it in `SHA256SUMS.txt`. The package installs `/usr/bin/oxypixel`, `docs/linux/oxypixel.desktop`, a 32×32 icon exported from `installer/oxypixel.ico`, and an `application/x-oxypix` mime file. Depends includes `libgl1`, `libx11-6`, `libxkbcommon0`, `libxkbcommon-x11-0`, and `libxcb1`, plus the libraries the binary links. `libxkbcommon-x11-0` is required: without it the window panics while loading `libxkbcommon-x11.so.0`. `PackageKind::LinuxDeb` is that asset when the running program is `/usr/bin/oxypixel`. `LinuxArchive` stays `oxypixel-<version>-linux-x86_64.tar.gz`, so the extra release file does not change the portable update. The names `oxypixel-0.3.2-windows-x86_64.exe`, `OxyPixel-0.3.2-Setup.exe`, and `oxypixel-0.3.2-linux-x86_64.tar.gz` are unchanged. RPM, AppImage, and Flatpak are still only described. The release workflow was not dispatched.

### OX-P3

**A window check that does not need a new control protocol.** Status: done.

`cargo test` composites the built-in example (`Editor::demo`) with `Document::composite`, the same path the canvas texture and PNG export use. Each frame is checked against a SHA-256 in `crates/oxypixel-core/tests/golden/demo-composite.txt` and the golden PNG beside it. The test runs on Ubuntu and Windows CI. It does not open a window, a dialog, or a command server.

Refresh the fixtures after an intentional change to the example or the composite:

```bash
OXYPIXEL_UPDATE_DEMO_GOLDEN=1 cargo test -p oxypixel-core --test demo_composite -- --nocapture
```

Review the PNG diff, run the test again without the variable, and commit `crates/oxypixel-core/tests/golden/`. CI leaves the variable unset. If it is set there, the test fails instead of rewriting the golden.

### OX-P4

**Palette import and export.** Status: done.

The document palette can be replaced from a file and written back out. GIMP `.gpl`, Lospec-style `.hex` (one `RRGGBB` per line, or `RRGGBBAA` when the color has alpha), and JASC `.pal` are parsed in `oxypixel-core`. A `.txt` file is the same hex list. Comments and blank lines are skipped. A file with no colors, a bad line, or more than 256 colors returns an error and does not change the document. Import is one undo step. The palette panel and the File menu use the same path dialog as other files. `export-palette` and `import-palette` print failures and exit. They do not open a window or a modal dialog.

### OX-P5

**A crash and error log in the app data directory.** Panics and errors are only printed today. A later task could append them to a log under the same directory recent files already use. Headless failures would stay on stderr, with no dialog.

### OX-P6

**A Windows launch smoke test in CI.** Status: done. Area: `.github/workflows/ci.yml`, `crates/oxypixel/src/native.rs`.

After the release build, the `windows-latest` job starts `target\release\oxypixel.exe`, waits up to 45 seconds for a window titled `OxyPixel - …`, closes it the way a user would, and expects exit code 0 within 15 seconds. A process that exits during startup, never shows that window, or does not exit after the close fails the step with its stderr. `OXYPIXEL_NO_DIALOG=1` turns the startup error box into a stderr line, since nothing on the runner can click it. `APPDATA` points at a temp folder, so recent files and recovery copies stay off the runner profile. There is no control socket; the step uses the window title and an ordinary close.

### OX-P7

**Snap zoom to whole pixels, and keep the grid visible at Fit.** Status: done. Area: `crates/oxypixel/src/canvas.rs`.

Zoom in, zoom out, and the wheel step through whole-pixel scales from 1× to 64×. Below 1× the steps are 1/2, 1/3, 1/4, 1/5, 1/6, 1/8, and 1/10. The pointer stays over the same document pixel. Fit (Ctrl+0) still frames the canvas, on the largest of those scales that fits in the panel. The grid draws from 2× upward. From 2× to 4× the lines are every two or four document pixels, so they stay a few screen pixels apart, and a span of more than 200 cells uses a lighter line. `next_clean_zoom`, `fit_zoom`, and `grid_draw` are unit tested.

### OX-P8

**Preview the brush in the color that will be painted.** Status: done. Area: `crates/oxypixel/src/canvas.rs`.

Pencil, line, rectangle, and ellipse fill the hover stamp with the primary color, or the secondary color while the right button is down. The fill is translucent, with a contrasting edge. The eraser fills the stamp with a light tint. Fill, eyedropper, and select keep a one-pixel outline. `hover_paint` is unit tested.

### OX-P9

**Show the selection, then flip it or turn it by 90 degrees.** Status: done. Area: `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel-core/src/draw.rs`, `crates/oxypixel/src/app.rs`.

Edit → Flip horizontal, Flip vertical, Rotate 90 clockwise, Rotate 90 counter-clockwise, and Rotate 180 each take one undo step. Shortcuts are Shift+H, Shift+V, Shift+., Shift+,, and Ctrl+Shift+.. A selection transforms only that rectangle on the active cel. With nothing selected, the command uses every pixel of the active cel (this layer on this frame). Other layers and frames stay put, and the canvas size does not change. Flips and a 180° turn keep the rectangle. A 90° turn swaps its width and height and keeps the same center. If the turned rectangle fits and still touches the cel, it shifts fully onto the cel. If it is wider or taller than the cel, the centered pixels that land outside are dropped and the selection covers only what remains. A turn that misses the cel changes nothing. The marquee is a dark edge plus an amber edge, and the status line prints the selection size. Resampling a selection to a new size is still a separate task.

### OX-P10

**Print the shortcut on each tool button.** Status: done. Area: `crates/oxypixel/src/app.rs`.

Each tool button draws its letter (B, E, G, I, L, R, O, M) in the corner of the icon. The tooltip and Help → Shortcuts still list the same keys.

### OX-P11

**Thumbnails on the frame strip, onion skin for the next frame, and the full duration range.** Status: done. Area: `crates/oxypixel/src/framestrip.rs`, `crates/oxypixel/src/canvas.rs`.

Each frame button shows a small composite of that frame and its number. The hover tip includes the duration. Frame names are not edited: a version 1 `.oxypix` file has no frame-name field, and this change does not add one. Onion skin can ghost up to eight frames before the current one and eight after it. Earlier ghosts lean rose and later ghosts lean teal when Tint is on, and Opacity scales them, with farther frames dimmer. Frame 1 does not show the last frame, and the last frame does not show frame 1. The duration slider runs from 20 ms to 10000 ms, on a logarithmic scale so both a short frame and the file-format maximum are reachable.

### OX-P19

**A preview of the whole image beside the canvas.** Status: done. Area: `crates/oxypixel/src/preview.rs`, `crates/oxypixel/src/session.rs`.

View → Show preview opens a resizable panel between the canvas and the layer list. It draws the composited frame at the largest integer scale that fits, or one screen pixel for several document pixels when the frame is larger than the panel. The picture updates as you draw. With more than one frame, Play walks the durations inside the panel and leaves the edited frame where it is; Pause shows the frame you are on. A click or drag pans the main canvas so that spot is centered, and an amber rectangle marks the canvas viewport. Tiled view repeats that canvas 3 by 3 on the main view; the preview stays one composite, and the amber rectangle stays inside that single tile. Hide preview closes the panel. The choice is stored in `view.txt` next to recent files and is restored on the next launch. A composite is built once per frame until the pixels change. A stroke rebuilds only the active frame. Playback reuses the frames it already built.

### OX-P12

**A recovery file after a crash.** Status: done. Area: `crates/oxypixel/src/recovery.rs`, `crates/oxypixel/src/app.rs`.

A dirty document is copied to a recovery `.oxypix` in the app data directory, next to `recent.txt`, about once a minute and after 30 edits. The copy is written on a background thread as a temp file plus a rename, after the stroke or slider drag has finished, and it never uses the user's own path. An untitled drawing is included. The next launch lists each copy with its name and the time it was saved, and asks to restore or discard it. A successful save, Discard on the unsaved-changes dialog, or a close with a clean document removes that copy. File → Turn off autosave recovery stores the choice in the same data directory. Export commands, unit tests, and a headless run do not write a recovery file and do not open a dialog.

### OX-P13

**Drop the extra corner on a one-pixel pencil stroke.** Status: done. Area: `crates/oxypixel-core/src/draw.rs`, `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel/src/app.rs`.

Pixel-perfect is off until the user turns it on (checkbox, or P). A one-pixel pencil stroke that visits both pixels of a diagonal step drops the corner. A larger brush keeps every pixel. The line tool is unchanged. Mirror stamps follow the same cleaned path. The stroke is still one undo step. The rule is tested in `draw.rs`.

### OX-P14

**Wrap a stroke around the canvas for a tile.** Status: done. Area: `crates/oxypixel-core/src/draw.rs`, `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel/src/canvas.rs`.

Wrap is off until the user turns it on (checkbox, or T). Pencil, eraser, line, rectangle, ellipse, and fill that leave an edge continue on the opposite edge, including the shape preview. Mirror still reflects across the canvas center. Tiled view (checkbox, or Shift+T) draws the same cel 3 by 3 so the seam is visible. Reference overlays stay on the real canvas and are not repeated. Export writes that one cel. The wrap math is tested in `draw.rs`.

### OX-P15

**Show enough of a recent file to tell two of them apart.** Status: done. Area: `crates/oxypixel/src/recent.rs`, `crates/oxypixel/src/app.rs`.

Open recent keeps ten paths. A repeated file name shows the parent folder, and a longer suffix when those folder names also match. A path that is not a file is labeled "(missing)". Clear recent empties the list and writes it back. The full path stays on the hover tip. The label and the ten-path cap are unit tested.

### OX-P16

**Reorder swatches, and optionally snap strokes to the palette.** Status: done. Area: `crates/oxypixel-core/src/color.rs`, `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel/src/app.rs`.

Drag a swatch to a new index. That move is one undo step. Shift-click a second swatch and Ramp fills the inclusive span with N steps, including both ends, also one undo step. Snap (S) is off until the user turns it on. Pencil, fill, line, rectangle, and ellipse then stamp the nearest palette color. The eraser still clears, and the eyedropper still picks the pixel. The chosen color is unchanged. `.gpl`, `.hex`, and `.pal` round-trip the new order (JASC still stores opaque colors). The `.oxypix` version stays 1, and the document stays RGBA. Ramp generation and nearest-color snapping are tested in `color.rs`.

### OX-P17

**A reference image that export skips.** Status: done. Area: `crates/oxypixel-core/src/reference.rs`, `crates/oxypixel/src/refsview.rs`.

File → Load reference, or a dropped JPEG, keeps the picture beside the document. Up to eight can be shown. Each one is either an overlay (Ctrl+drag to move it, scale from 1/64 to 8 and opacity sliders, a typed width or height in canvas pixels that keeps the aspect ratio, Fit to canvas, lock, show or hide, behind or above the art) or a floating panel. A new reference starts behind the art. Without Ctrl, every tool draws over a reference, so tracing does not need Lock. Sample, off by default, lets the eyedropper read that image. Copy onto layer is the explicit way those pixels become a layer, and only then do export, merge, and composite include them. Tiled view repeats the cel and leaves the reference where it was placed. The `.oxypix` version stays 1. Version 1 rejects trailing bytes, so Save writes paths and placement to `<file>.oxypix.refs` next to the document. An image inside the document's folder is stored relative to it. A reference change marks the document unsaved, and Discard drops it. A dropped PNG still imports, as before; a PNG reference comes from File → Load reference. A sidecar that could not be read is left on disk until the references change. The pixels stay in the PNG or JPEG. A missing sidecar means there are no references. A missing image keeps the placement and is marked missing.

### OX-P18

**Blend modes on a layer, once the file format allows them.** Each layer already has an opacity, and compositing is source-over. A later task could add a few blend modes, such as multiply, screen, and add, stored on the layer. That needs a new `.oxypix` version, so it waits until a human accepts the format change.

### OX-P20

**Keep references in the recovery copy.** Status: done. Area: `crates/oxypixel/src/recovery.rs`, `crates/oxypixel/src/app.rs`, `crates/oxypixel-core/src/reference.rs`.

Each recovery copy gets a `{id}.oxypix.refs` beside it, written in the same background job and renamed into place before the meta file that marks the copy complete. An empty list is written too, so a restore never falls back to older references. Moving, scaling, or adding a reference counts as an edit for autosave, even when no pixel changed. Restore loads that sidecar and marks the references unsaved, so the next Save writes them into the user's folder; a copy from 0.6.1 or earlier, which has no sidecar, still uses the document's own. Discarding a copy, saving, or a clean close removes the sidecar with the copy. Reference paths are kept absolute in memory, so a document opened by a relative path (`oxypixel art.oxypix`) still finds its images from the recovery folder or after a Save As elsewhere. Tested in `recovery.rs` and `reference.rs`, and by a kill-and-restore run of the window.

### OX-P21

**Keep the palette marks on the right swatch after undo.** Status: done. Area: `crates/oxypixel/src/app.rs`, `crates/oxypixel-core/src/editor.rs`.

When an undo or redo changes the palette, the selected swatch and the ramp end follow their color: the same index if it still holds that color, otherwise the nearest index that does, the earlier one on a tie. A ramp end whose color is gone is cleared; a selected swatch whose color is gone stays where it was, clamped to the palette. `follow_palette_mark` is unit tested, and the P16 tests now assert that a swatch dropped on itself records no step and that Snap reports the nearest swatch.

### OX-P22

**Fill tolerance and a global fill.** Status: done. Area: `crates/oxypixel-core/src/draw.rs`, `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel/src/app.rs`.

With the fill tool, the tools panel shows Global fill and a Tolerance slider from 0 to 255. A pixel matches when each RGBA channel is within the tolerance of the clicked pixel, so 0 is the exact fill from before. Global recolors every match on the active cel instead of the connected area. Both stay inside the selection; wrap applies to a connected fill, and Snap picks the ink. Each fill is one undo step, and both settings are session state, not saved in the file. `fill_matches` and both modes are tested in `draw.rs`, and the undo step in `editor.rs`.

### OX-P23

**Magic wand, and selections that are not rectangles.** Status: done. Area: `crates/oxypixel-core/src/selection.rs`, `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel-core/src/draw.rs`, `crates/oxypixel/src/canvas.rs`, `crates/oxypixel/src/app.rs`.

The selection is its bounding rectangle plus an optional mask, one flag per pixel of that rectangle; a marquee and Select All store no mask. `Editor::set_selection` is the only place it changes, and it trims the mask to what it selects, drops a mask that fills its rectangle, and keeps the outline edges so the canvas does not trace them every frame. The wand (W) selects what `fill_region` reaches from the clicked pixel on the active cel, with the fill tool's Tolerance, Global, and wrap; the bucket fills the same region. Shift adds a marquee or wand area and Alt cuts it out (Alt no longer picks a color with these two tools); combining works on the canvas, so the parts of a selection hanging off it are dropped. Moving, nudging, copying, and clearing touch only selected pixels, and a move or paste leaves the unselected pixels of the rectangle as they were. Flips and turns turn the mask with the pixels. Undo snapshots carry the mask, and a selection change by itself is still not an undo step. The in-memory clipboard keeps the mask; the system clipboard gets the rectangle with clear pixels outside it. The selection stays session state, not saved in the file. The mask helpers are tested in `selection.rs`, the wand, Shift/Alt, masked move, clear, clip, paste, and flip in `editor.rs`, and the modifier mapping in `canvas.rs`. A lasso can build on the same mask.

### OX-P24

**Shade along the palette.** Status: done. Area: `crates/oxypixel-core/src/color.rs`, `crates/oxypixel-core/src/editor.rs`, `crates/oxypixel/src/app.rs`.

Shade (checkbox, or D) turns the pencil into a shading brush. Left button replaces each pixel with the next swatch after its color, right button with the swatch before it. A color not in the palette, or one at the end it would step past, stays as it is. The stroke is rewritten from the cel as it was before the stroke, so going back over a pixel shades it once, and a stroke is one undo step. Brush size, pixel-perfect, mirror, wrap, and the selection still apply; the eraser is unchanged. `shade_step` is tested in `color.rs` and the stroke in `editor.rs`.

### OX-P25

**Spritesheet layout and a JSON description.** Status: done. Area: `crates/oxypixel-core/src/io/raster.rs`, `crates/oxypixel/src/cli.rs`, `crates/oxypixel/src/app.rs`.

Export spritesheet takes Columns (0 is one row, as before), Padding (clear pixels between cells, up to 256), and Scale, and can also write `<name>.json`. That file is a JSON array sheet: one entry per frame with its `frame` rectangle, `sourceSize`, and `duration` in milliseconds, then `meta` with the image name, the sheet size, and the scale. `export-sheet` takes `--columns`, `--padding`, `--scale`, and `--json`, and still fails with an exit code, not a dialog. Width and height are each capped at 16384 and the area at 64 Mi pixels, so a grid can be no larger than the widest one-row sheet already was. A one-row sheet is byte-for-byte what it was. `spritesheet_grid`, the PNG, and the JSON are tested in `raster.rs`, and the options in `tests/cli.rs`.

### OX-P26

**Scale a selection.** P9 flips and turns a selection; resizing it is still missing. A later task could scale the selected pixels by a whole-number factor up or down, or to a typed size, with nearest-neighbor sampling only. It is one undo step, and pixels that land outside the cel are dropped as in P9.

### OX-P27

**`.oxypix` version 2.** Frame names (P11) and blend modes (P18) both need a new version, because version 1 rejects trailing bytes. A later task could design version 2 once and fold those fields in, with room for later fields. It would also decide whether references stay a sidecar. Version 1 files must still open, and saving writes version 2 only when the document uses a version 2 field. This needs a human to accept the format change before work starts. Update [FORMAT.md](FORMAT.md) and the parse limits in the same change.

### OX-P28

**OxyPack, a companion sprite packer.** Status: done. Area: `crates/oxypack/`, `crates/oxypixel/src/packview.rs`.

Done: the `oxypack` library and command described in the scorecard, and the editor's File, then Pack sprites… window, which calls the same `pack_files` and `write_atlas`. Tested in `crates/oxypack/src/pack.rs`, `aseprite.rs`, `files.rs`, `main.rs`, `tests/cli.rs`, and `crates/oxypixel/src/packview.rs`. It stays in this repository and uses only the core's public API. Releases ship `oxypack-<version>-windows-x86_64.exe` (listed in `SHA256SUMS-windows.txt`, not in the updater's `SHA256SUMS.txt`), `oxypack` in the Linux tar.gz, and `/usr/bin/oxypack` in the `.deb`. The Windows installer does not include the command; installer users have the window.

Still open, each needing a decision: true group blending and blend modes other than Normal for `.aseprite` files, tilemap layers, and tags for `.oxypix` files, which have no place to store them until `.oxypix` version 2 (OX-P27).

## Human only

### OX-H1

**Authenticode signing and cutting a release.** Needs a certificate and a version decision. Agents do not push `v*` tags or run the release workflow to try this. The steps that already exist are in [docs/BUILD-WINDOWS.md](BUILD-WINDOWS.md) and `.github/workflows/release.yml`.

A release starts in one of two ways, and each creates the `v<version>` tag on the commit it builds: push the tag on the version-bump merge commit, or run the Release workflow on `main` (Actions → Release → Run workflow). The version comes from `Cargo.toml`. 0.6.0 was released the second way, because a Claude Code cloud session could push branches but not tags. Use only one of the two for a release, so it builds once.
