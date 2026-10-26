# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The Windows installer also installs the `oxypack.exe` command-line packer beside `oxypixel.exe` in `%LOCALAPPDATA%\OxyPixel`, and the uninstaller removes it. Setup does not change `PATH`. The standalone `oxypack-<version>-windows-x86_64.exe` download stays.

## [0.8.0] - 2026-10-26

### Added

- OxyPack reads Aseprite `.aseprite` and `.ase` files: RGBA, grayscale, and indexed, with layer and cel opacity, hidden and reference layers, groups, linked cels, and z-index. Each frame becomes a sprite with its duration. The reader was written from Aseprite's published file format description and uses no Aseprite code. Blend modes other than Normal are drawn as Normal and tilemap layers are skipped, with a note on stderr.
- File, then Pack sprites… opens OxyPack in a window: add files or folders, choose the packer, padding, width limit, trim, dedupe, power-of-two sizes, and JSON, press Pack to see the atlas, then save it. It makes the same PNG and JSON as the `oxypack` command.
- OxyPack writes Aseprite tags to `meta.frameTags` in the JSON, with their direction and repeat count, numbered across the whole atlas when several files are packed together. The list is empty when no input has tags.
- OxyPack can pack with MaxRects (`--packer maxrects`), which fills the gaps between rows and usually gives a shorter atlas. The row packer stays the default, so existing atlases do not move.

## [0.7.5] - 2026-10-26

### Added

- OxyPack (`oxypack`), a companion command-line tool that packs sprites from PNG, BMP, GIF, and `.oxypix` files, or folders of them, into one atlas PNG with a JSON hash for game engines. Each animation frame becomes a sprite with its duration. Options cover padding, a width limit, trimming to the ink, storing repeated frames once, and power-of-two sizes. Releases ship `oxypack-<version>-windows-x86_64.exe`, `oxypack` in the Linux archive, and `/usr/bin/oxypack` in the `.deb`.

## [0.7.0] - 2026-10-26

### Added

- The fill tool has a Tolerance slider (0 to 255) that also takes near colors, and a Global fill option that recolors every matching pixel on the layer instead of one connected area. Both stay inside the selection, and each fill is one undo step.
- Shade (D) turns the pencil into a shading brush. The left button steps each pixel to the next swatch in the palette and the right button to the one before. A pixel changes once per stroke, colors not in the palette are left alone, and a stroke is one undo step.
- Spritesheet export can lay frames out in a grid (Columns), leave clear pixels between them (Padding), and write a `.json` beside the PNG with each frame's rectangle and duration, in the JSON array sheet layout game engines read. `export-sheet` takes `--columns`, `--padding`, `--scale`, and `--json`. A one-row sheet is unchanged.
- Magic wand (W) selects the area the fill tool would reach, using the same Tolerance and Global fill settings. Selections can now be any shape: Shift adds a marquee or wand area to the selection and Alt cuts it out. Move, nudge, flip, rotate, copy, paste, clear, and drawing all keep to the selected pixels, the outline follows their edge, and undo restores the shape.

### Fixed

- Crash recovery keeps reference images. Their placement is saved with each recovery copy, and moving a reference alone now triggers autosave. Restoring brings them back where they were, and the next Save writes them beside the document.
- A document opened by a relative path, such as `oxypixel art.oxypix` from a terminal, no longer loses its reference images after a Save As into another folder.
- After undoing or redoing a swatch move or a ramp, the selected swatch and the ramp end stay on the color they marked instead of the slot.

### Changed

- The window draws with wgpu: Direct3D 12 on Windows, with the WARP software renderer when there is no GPU, and Vulkan on Linux. If wgpu cannot start, OxyPixel falls back to OpenGL as before. This fixes "egui_glow requires opengl 2.0+" on Windows virtual machines, Remote Desktop sessions, and machines without an OpenGL 2 driver. The `.deb` recommends `libvulkan1` and `mesa-vulkan-drivers`.
- The Windows CI job starts the release build, waits for its window, and closes it, so a build that cannot open its window fails the pull request. `OXYPIXEL_NO_DIALOG=1` keeps a startup error on stderr instead of in a message box.

## [0.6.1] - 2026-10-24

### Fixed

- Drawing over a reference image works. A press on a reference used to grab it and move it, so the pencil could not trace it unless Lock was on. Now Ctrl+drag moves a reference, and without Ctrl every tool draws.
- A new reference starts behind the art, so strokes are not hidden under it.
- The empty black strip between the tools panel and the canvas is gone. The brush slider's label made that row wider than the panel; it now sits on its own line, and the tool hint no longer cuts off.

### Added

- Fit to canvas scales a reference to the document and centers it. The Scale slider now reaches 1/64, so a 2000-pixel image fits a 32-pixel canvas.
- A reference's size can be typed in canvas pixels. Change the width or the height and the other follows, so the image keeps its shape.

## [0.6.0] - 2026-10-24

### Added

- The frame strip shows a thumbnail of each frame. Onion skin can ghost several frames before and after the current one, with opacity and a tint. It does not wrap the last frame onto the first.
- View → Show preview opens a panel of the whole composited image beside the canvas. It can play the frames at their durations, a click or drag there pans the canvas, and an amber rectangle marks the viewport. The panel remembers whether it was left open. Tiled view repeats the canvas on the main view; the preview stays one tile.
- File → Load reference, or a dropped JPEG, shows that image on the canvas or in a floating panel. It can be moved, scaled, faded, locked, hidden, and drawn behind or above the art. The eyedropper ignores it unless Sample is on. Copy onto layer is what puts those pixels into the document. Tiled view repeats the cel, not the reference. Save writes the paths and placement to `<file>.oxypix.refs` beside the document, relative to its folder when the image is inside it. They are not a field in the `.oxypix` file, and export does not include them. A reference change counts as an unsaved change.
- Pixel-perfect pencil, off until you turn it on (P), drops the extra corner of a one-pixel diagonal step. A larger brush and the line tool are unchanged. Mirror follows the cleaned stroke, and the stroke is still one undo step.
- Wrap (T) continues pencil, eraser, fill, lines, and shapes that leave an edge on the opposite edge, including the shape preview. Tiled view (Shift+T) draws the canvas 3 by 3 so the seam is visible. Export still writes one copy.
- Palette swatches can be dragged into a new order, one undo step. Ramp fills the span between two swatches with N steps, including both ends. Snap (S), off until you turn it on, makes pencil, fill, and shapes stamp the nearest palette color. The eraser still clears. GIMP `.gpl`, hex `.hex`, and JASC `.pal` keep that order.

### Changed

- The frame duration slider runs from 20 ms to 10000 ms, which is the range stored in a `.oxypix` file.

## [0.5.0] - 2026-10-23

### Added

- Autosave keeps a recovery `.oxypix` in the app data folder, about once a minute and after a run of edits. It does not overwrite the open file. The next launch can restore or discard that copy, including a drawing that was never saved. File can turn autosave off. Saving, or discarding changes, removes the recovery copy.
- Each tool button shows its shortcut letter.
- File → Open recent keeps ten documents. A repeated file name shows the parent folder, a missing file is marked, and Clear recent empties the list.
- Edit can flip the selection horizontally or vertically, or turn it 90° clockwise, counter-clockwise, or 180°. With nothing selected the same commands change the active cel. Each one is a single undo step. Shortcuts are Shift+H, Shift+V, Shift+., Shift+,, and Ctrl+Shift+.. A 90° turn of a non-square rectangle swaps its sides and stays centered; pixels that do not fit on the cel are dropped.

### Changed

- The selection outline is a dark edge plus an amber edge, and the status line shows the selection size.
- Zoom in, zoom out, and the wheel step through whole-pixel scales (1×, 2×, 3×, and so on, up to 64×). Below 100% the steps are 1/2, 1/3, 1/4, 1/5, 1/6, 1/8, and 1/10. Fit still frames the canvas, on the largest of those scales that fits.
- The pixel grid draws below 600% when a cell is still large enough to read. From 200% to 400% the lines are coarser, and a wide view uses a lighter line.
- The brush hover mark is filled with the color that will be painted. The right button previews the secondary color. The eraser uses a light tint. Fill, pick, and select stay a one-pixel outline.
- Ubuntu CI installs the MinGW compiler from the Ubuntu archive. It does not run `apt-get update` against the Azure mirror, which was hanging until the job timed out.

## [0.4.0] - 2026-10-21

### Added

- Copy puts the selection on the system clipboard as a straight-alpha image and keeps the in-memory patch. Paste paints a system image when one is present, or that patch when the clipboard has none. Ctrl+C and Ctrl+V still run when the text clipboard is empty. On Windows the image is a PNG and a 32-bit DIB. On Linux it is `image/png` while OxyPixel is open.
- Linux releases include `oxypixel-0.4.0-linux-x86_64.deb` beside the tar.gz. The package installs `/usr/bin/oxypixel`, a desktop entry, a 32×32 icon, and the `.oxypix` file type. Check for updates downloads that package when the program is `/usr/bin/oxypixel`, and still downloads the tar.gz for an unpacked copy. Install the downloaded package with the package manager.
- The palette can be imported and exported as GIMP `.gpl`, a hex list (`.hex` or `.txt`, one `RRGGBB` per line, or `RRGGBBAA` when the color has alpha), and JASC `.pal`. Import replaces the palette and is one undo step. `export-palette` and `import-palette` do the same from the command line.
- `cargo test` compares the built-in example's composited frames with a stored image hash. It does not open a window.

### Changed

- The README is written for people downloading and running the editor, with screenshots of the window.
- Tests lock the update filenames to the version of this copy, and check that recent files use the platform data directory.
- `make check-windows` runs clippy for the Windows build, and the Ubuntu CI job runs that check.
- A pull request runs CI once. A newer push on the same branch cancels the run it replaces. Documentation-only changes skip the test jobs. Tag pushes stay on the release workflow.

## 0.3.2

- Help, then Check for updates, can move an existing install to this release. The download is kept only when its SHA-256 matches `SHA256SUMS.txt` from the same release.
- An installed Windows copy opens `OxyPixel-0.3.2-Setup.exe`. A portable Windows copy replaces its exe and restarts. A Linux copy replaces `oxypixel` in its folder when that folder can be written, then restarts.
- The release notes in that dialog are the text of this section.

## 0.3.0

- Drag a selection to move those pixels, or nudge them one pixel with the arrow keys. Ctrl+C and Ctrl+V copy between frames. Undo puts the marquee back with the pixels.
- Canvas size can grow or crop. The offset says where the current top-left pixel moves, and Top-left or Center set that offset.
- Mirror H and Mirror V stamp pencil, eraser, and shape strokes on the opposite side of the canvas.
- PNG and spritesheet export can be 2× or 4× nearest-neighbor, so the pixels stay sharp.
- Tool buttons use an original icon set. The shortcut letter stays on the tooltip.
- The window opens on the built-in example, already fitted. File remembers recent `.oxypix` documents. Dropping a PNG, BMP, or GIF imports it, and dropping a `.oxypix` file opens it.
- Linux Browse uses the system file dialog through the desktop portal. The path field remains when that dialog does not return a file.

## 0.2.1

- Help, then Check for updates, asks GitHub for the latest public release. OxyPixel does not open a network connection until that command is chosen.
- A downloaded file is kept only when its SHA-256 matches `SHA256SUMS.txt` from the same release. Files from any other site are refused.
- A portable Windows copy restarts into the new executable. An installed copy opens the setup program, and the installer is not silent because the build is still unsigned. A Linux copy replaces `oxypixel` in its folder when that folder can be written, then restarts.

## 0.2.0

Editor:

- File, Edit, and View menus show the shortcuts they already used.
- Undo and Redo sit on the tool panel and stay disabled when there is nothing to undo or redo.
- Each tool has a one-line hint. The hover mark is a brush stamp for pencil, eraser, and shapes, and a single pixel for fill, pick, and select.
- Space still pans, and the cursor changes to a hand. While playback is running, the canvas says drawing is paused.
- The status line shows the composited color under the pointer, with zoom controls and a Grid checkbox beside it.
- Frame numbers scroll instead of pushing the other controls off the window.
- Enter confirms New, Open, Save, the unsaved-changes dialog, and a hex color.
- Hiding a layer no longer switches the layer you are drawing on.
- Palette swatches note that a right-click sets the secondary color.

Fixes:

- Headless export failures print to stderr and exit. They no longer open a Windows message box, which had left `missing_file_fails` waiting on GitHub runners until the job was cancelled.
- That test now stops the process if it is still running after 15 seconds.
- CI jobs time out after 15 minutes. The release job times out after 20 minutes.

Windows installer:

- The setup wizard uses the usual page order: welcome, MIT license, components, folder, install, and finish.
- Welcome and header artwork is the OxyPixel mark. The wizard is DPI-aware, and the window title is `OxyPixel 0.2.0 Setup`.
- The editor is required. The desktop shortcut stays optional. The folder page says the install does not need administrator rights.
- The finish page says the build is unsigned and can launch OxyPixel.
- Add/Remove Programs records the install size and a quiet uninstall command.
- Installer artwork and the program are loaded from the script directory. `makensis` switches to that directory, so the build works when it is started from the repository root.

Release files:

- `OxyPixel-0.2.0-Setup.exe`, the per-user NSIS installer
- `oxypixel-0.2.0-windows-x86_64.exe`, the portable Windows executable
- `oxypixel-0.2.0-linux-x86_64.tar.gz`, the Linux x86_64 executable and a short run note

These Windows files are unsigned. The Linux archive is the program only. It is not a `.deb`, RPM, AppImage, or Flatpak.

## 0.1.0

- First usable editor: canvas, drawing tools, color, layers, undo, shortcuts
- Animation frames, playback, onion skin, GIF export, spritesheet export
- `.oxypix` version 1, plus PNG, BMP, and GIF import and export
- Windows per-user NSIS installer and a static-CRT `oxypixel.exe`
- Unsigned, Authenticode-ready binaries when no certificate is configured
