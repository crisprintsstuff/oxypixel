# Installer

`oxypixel.nsi` is an NSIS 3 script. It builds `dist/OxyPixel-0.1.0-Setup.exe`.

The wizard is the standard Modern UI sequence: welcome, MIT license, optional desktop shortcut, install folder, progress, and a finish page that can launch OxyPixel. The sidebar and header art are `welcome.bmp` (164×314) and `header.bmp` (150×57). The installer is per-user, includes an uninstaller, and is unsigned until someone runs `signtool` as described in [../docs/BUILD-WINDOWS.md](../docs/BUILD-WINDOWS.md).

Build it from the repository root:

```bash
./scripts/build-windows.sh
./scripts/build-installer.sh
```

On Windows, after `cargo build --release -p oxypixel`, copy the exe into `dist` and run `makensis installer\oxypixel.nsi`.
