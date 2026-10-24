# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
