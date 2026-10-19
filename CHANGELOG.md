# Changelog

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
