# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The Flatpak manifest builds from the 0.9.6 release archive.

## [0.9.6] - 2026-10-28

### Fixed

- A stroke, line, or selection dragged at low zoom started a few pixels after the point where the button went down, because the drag only counted once the pointer had moved. It now starts at the press.

### Added

- An AppImage, `OxyPixel-<version>-x86_64.AppImage`, is built and published with each release, with its checksum in `SHA256SUMS.txt`. Check for updates replaces it in place. A link to it named `oxypack` runs OxyPack.
- A Flatpak manifest, `packaging/flatpak/io.github.crisprintsstuff.OxyPixel.yml`, with AppStream metainfo and a 256×256 icon. It builds a local Flatpak from the release archive. A Flatpak copy leaves updates to Flatpak.
- Window tests: the whole app runs headlessly in the test suite, and tests click, drag, and press keys in it the way a person would.
- A live MCP bridge. With Edit, then Settings…, AI assistants, Live MCP bridge turned on, the MCP tools work on the document open in the window when they are called without a `path`. Each change is one undo step in the window, and a change is refused if the document was edited by hand after the assistant read it. The bridge listens only on 127.0.0.1 and needs a random token from a file only the user can read. It is off unless turned on.

## [0.9.5] - 2026-10-28

### Added

- Layer blend modes. Pick one under a layer's opacity slider: Multiply, Screen, Overlay, Darken, Lighten, Color dodge, Color burn, Hard light, Soft light, Difference, Exclusion, Hue, Saturation, Color, Luminosity, Addition, Subtract, or Divide. The canvas, preview, thumbnails, merge down, PNG, GIF, BMP, and spritesheet exports, OxyPack, and the MCP server (`set_layer` takes `blend`) all use them.

- Layer groups. Group puts the selected layer in a new group, In moves a layer into the group just below it, and Out moves it back out. Up, Down, Copy, and Delete carry a whole group, and hiding a group hides what is inside it. A group is drawn as one picture and mixed in with its own opacity and blend mode, the way Aseprite composites groups. Drawing tools do nothing on a group. The MCP server's `add_layer` takes `group`, `set_layer` takes `group_into` and `group_out`, and `document_info` lists each layer's group and parent.

- Edit, then Scale… resizes the selection, or the whole layer, to a typed size or by ½×, 2×, 3×, or 4× with nearest-neighbor pixels, keeping its top-left corner. One undo step.
- Edit, then Outline in primary color, Outline with corners, and Drop shadow in second color paint the clear pixels around or behind the art on the active layer, inside the selection when there is one. The MCP `draw` tool has matching `outline` and `shadow` operations.
- Edit, then Replace color… changes every pixel of the primary color to the second color on the active layer, or on every layer, every frame, or both, inside the selection when there is one. One undo step. The MCP `draw` tool has a matching `replace` operation.
- Dither, under the tool options, makes the pencil paint a checkerboard of the primary and second colors that stays fixed to the canvas.
- Brush from selection, under the brush size, makes the pencil stamp the selected pixels (up to 64×64) in their own colors and the eraser clear their shape. The canvas previews it under the pointer, and Square goes back to the normal brush.
- Export spritesheet can also write a Godot 4 SpriteFrames resource (`.tres`) with one animation per tag, using each frame's duration, and Unity import settings (`.png.meta`) that cut the sheet into one point-filtered sprite per frame. Exporting again keeps the Unity file's `guid`, so references in a Unity project survive.
- One sheet per tag and One GIF per tag write `<name>_<tag>.png` or `.gif` for each tag. A tag's GIF plays its frames in the tag's direction and as many times as it repeats.
- Tilemap layers. Tilemap… in the layer panel adds a layer that is a grid of tiles from a new or existing tileset. Drawing on it changes the tiles under the pointer, and every copy of a tile changes too; drawing on an empty cell makes a new tile. Tile mode places, flips, empties, fills, and picks whole tiles. The Tiles panel shows the tileset, and Duplicate copies the selected tile into a new one. The canvas marks the tile grid. Frames, canvas resizing (by whole tiles), undo, and every export work with them, and the MCP `document_info` lists tilesets and each layer's tileset.
- File, then Open… opens Aseprite `.aseprite` and `.ase` files for editing, as does drag-and-drop and the command line. Layers, groups, visibility, opacity, blend modes, frame durations, tags, and the palette are kept; Save writes a new `.oxypix` and never changes the Aseprite file. Cel opacity is multiplied into the pixels, tilemap layers open as plain layers, reference layers are left out, and the status line says when any of that happened. The export commands read `.aseprite` files too.
- `export-sheet` takes `--godot`, `--unity`, and `--per-tag`, `export-gif` takes `--per-tag`, and the MCP `export` tool takes `godot`, `unity`, and `per_tag`.

### Changed

- A document with a group is saved with another required `.oxypix` section, with the same effect on older versions as blend modes.
- Tilesets and tilemap layers are saved in two more required `.oxypix` sections, `0x8003` and `0x8004`. OxyPixel 0.9.2 and earlier refuse such a file.
- A document with a layer that is not Normal is saved with a new required `.oxypix` section. OxyPixel 0.9.2 and earlier refuse such a file with "this file needs a newer OxyPixel" instead of drawing it wrong. Documents that only use Normal are saved exactly as before.

## [0.9.2] - 2026-10-27

### Added

- An MCP server for AI assistants: `oxypixel mcp`. Claude Desktop, Claude Code, and other Model Context Protocol clients start it and use its tools. The tools inspect documents, render a frame as an image, create documents, draw with the editor's tools, edit layers, frames, tags, and the palette, export PNG, BMP, GIF, or spritesheets, and pack sprites with OxyPack. Output files are never replaced without `overwrite`, and `--root DIR` keeps the tools inside one folder. It opens no window and no network connection. Setup is in the README.

- A Settings window: Edit, then Settings…, or Ctrl+,. It groups the app settings under General, Canvas, Onion skin, Export, and Updates. Changes apply right away and are kept for the next launch, and Reset to defaults puts them back. New settings:
  - interface scale, 75% to 200%
  - the size File, then New starts with
  - grid on or off at launch
  - a light checkerboard behind clear pixels as well as the dark one
  - onion skin frame counts, opacity, and tint
  - the PNG and spritesheet export scale

  Autosave recovery, automatic updates, the preview panel, and Clear recent files are there too. They are still in their menus as well.

### Fixed

- The Pack sprites window and the `oxypack` summary said "1 sprites" and "1 files". They now use the singular for one.

## [0.9.0] - 2026-10-26

### Added

- Automatic updates, off by default. Turn them on in the Help menu or the Check for updates window. OxyPixel then checks GitHub once a day at launch, downloads a newer release in the background, and checks it against `SHA256SUMS.txt`. An "Update … ready" button then appears in the menu bar. Nothing is installed until you click it, so unsaved work is never closed for you. The setting is kept in `view.txt` with the preview panel's.

- OxyPack draws Aseprite blend modes: Multiply, Screen, Overlay, Darken, Lighten, Color Dodge, Color Burn, Hard Light, Soft Light, Difference, Exclusion, Hue, Saturation, Color, Luminosity, Addition, Subtract, and Divide. When a file composites groups separately, a group is drawn as one image and then blended with its own opacity and mode, as Aseprite does. Before, every layer was drawn as Normal and a group's opacity multiplied its children's.

- OxyPack draws Aseprite tilemap layers from their tilesets, with x, y, and diagonal tile flips. Before, tilemap layers were skipped. A tileset kept in another file is still skipped, and a tile number the tileset does not have is drawn empty; both get a note on stderr.

- Frames can have names. Type one in the Name field beside the frame duration; typing is one undo step, and copying a frame keeps its name. The spritesheet JSON uses the name instead of "frame N", OxyPack names the sprite `<file> <name>`, and the frame strip's tooltip shows it. Names are saved in `.oxypix` version 2 (a new optional section), so a file without names or tags is still version 1.

## [0.8.6] - 2026-10-26

### Added

- Animation tags. Tags… under the frame strip names ranges of frames, with a direction (forward, reverse, ping-pong, or ping-pong reverse) and a repeat count; tagged frames get a mark on the strip. Each change is one undo step, and duplicating or deleting frames moves the tags with them. `export-sheet --json` and OxyPack write them to `meta.frameTags`.

### Changed

- A document with tags is saved as `.oxypix` version 2: version 1 plus sections after the palette, described in `docs/FORMAT.md`. OxyPixel 0.8.5 and earlier cannot open it. A document without tags is still saved as version 1, unchanged.

### Fixed

- The Linux editor in the 0.7.5, 0.8.0, and 0.8.5 releases needed glibc 2.43, so it did not start on Ubuntu 24.04, Debian 12, and other current systems, and the `.deb` would not install there. The Linux release is built on Ubuntu 24.04 again (it had moved with GitHub's `ubuntu-latest`), and packaging now fails if a binary needs a glibc newer than 2.39. `oxypack` was not affected.

## [0.8.5] - 2026-10-26

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
