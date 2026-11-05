Where PixelCraft is ahead
These are real gaps in OxyPixel, checked against both codebases, not just the READMEs.
Color and Aseprite files. PixelCraft stores RGB, grayscale, and indexed documents (ColorMode in src/model.rs) and round-trips linked cels, slices, user data, external tilesets, and ICC profiles. OxyPixel stays RGBA. It opens .ase / .aseprite and writes a copy back, but indexed and grayscale become pixels, tilemaps only survive when they already sit on the grid, and slices, links, and profiles are dropped.
Extra formats. PixelCraft imports animated GIF, APNG, lossless WebP, and OpenRaster, and exchanges finite Tiled .tmj / .tsj maps. OxyPixel is PNG, BMP, and GIF, plus its own .oxypix.
Tools. PixelCraft has lasso, hand, move, gradient, and five brush shapes (round, square, diamond, horizontal, vertical), up to 64 px. OxyPixel’s brush is a square, size 1–16. Its roadmap already notes that the selection mask can grow a lasso; it is not there yet.
Scripting. PixelCraft embeds Lua 5.4 and runs a tested slice of an Aseprite-like API: transactions, dialogs, themes, shortcut packs, languages, and dither matrices. OxyPixel has no plugin host. Its automation is a CLI plus an MCP server.
Autosave. PixelCraft backs up about every 15 seconds. OxyPixel does it about once a minute, or after 30 edits.
Where OxyPixel is ahead
PixelCraft does not have these:
•  Magic wand, shade brush, checker dither, and brush-from-selection.
•  Mirror drawing, wrap, and a 3×3 tiled view.
•  Reference images for tracing (PNG/JPEG overlays the export skips).
•  Palette files: GIMP .gpl, .hex, JASC .pal, plus ramp and snap-to-palette.
•  Multi-file atlas packing (oxypack: shelf and MaxRects, trim, dedupe, power-of-two). PixelCraft only packs the open sprite into a sheet.
•  Godot 4 SpriteFrames .tres and Unity .png.meta, including one file per animation tag.
•  Headless CLI (export-png, export-sheet, export-aseprite, import-sheet, and the rest).
•  MCP server and a live bridge into the open document.
•  Shortcut presets (OxyPixel, Aseprite, Photoshop), recent files, and a richer onion skin (up to eight frames each way, tinted). PixelCraft’s onion skin is the previous frame only.
•  A core library with no GUI, a window test harness, and unsafe forbidden. PixelCraft’s editor, Lua, and file IO sit in one package, and Lua/WebP need native libraries.
Catch-up list for OxyPixel
Do these in this order. Do not copy PixelCraft’s single-crate layout or drop wgpu.
1.  Raise the canvas limits. 1024×1024 and 32 layers will reject files PixelCraft already opens. The sheet exporter already allows 16384 and 64M pixels; the document caps in crates/oxypixel-core/src/limits.rs are the blocker.
2.  Indexed and grayscale modes. Biggest day-to-day gap versus both PixelCraft and Aseprite. Keep the RGBA composite path; store the mode and quantize on purpose.
3.  Stop flattening Aseprite on open. Keep linked cels, slices, and external tilesets instead of baking them into full-canvas RGBA. You already write .aseprite; the reader is the lossy side (OX-P30 / OX-P37).
4.  Lasso, then gradient and brush shapes. The mask from OX-P23 is the lasso. Brush shapes are a stamp change in draw.rs, not a new document model.
5.  APNG, WebP, and OpenRaster import. Same door as GIF import. Tiled .tmj export is the useful half of their tileset browser; your tilemap layers already exist.
6.  A small script host, not a full Aseprite clone. PixelCraft’s tested surface is the right scope: open document, transaction with undo, new layer/frame, flip, replace color, and a dialog. Lua fights unsafe_code = forbid. Either isolate the FFI in one crate or use a pure-Rust language. Do this after the file-format work. Themes and language packs can wait.
7.  Leave alone: the three-crate split, wgpu plus the OpenGL fallback, macOS, OxyPack, the CLI, and MCP. PixelCraft is behind on all of them.