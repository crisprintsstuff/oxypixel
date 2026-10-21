# Roadmap and scorecard

Status date: 2026-10-21. Version in tree: 0.3.2 (`Cargo.toml`). GitHub issues: none open. This file is the work list for [AGENTS.md](../AGENTS.md).

Rows are taken from the code and the docs. **Proposal** means the item is a reasonable next step, not a commitment and not an open issue. Do not start a proposal unless a human asks for it.

When you finish a task, change its status here in the same pull request.

## Current focus

Infrastructure, then small fixes that already have a place in the code.

1. [OX-011](#ox-011) — test that update filenames stay tied to `PackageKind`.
2. [OX-012](#ox-012) — test the recent-files directory on both platforms.

## Scorecard

### Done

Shipped in 0.3.2 and covered by tests or by the release workflow.

| Area | What exists | Where |
| --- | --- | --- |
| Tools | Pencil, eraser, fill, eyedropper, line, rectangle, ellipse, rectangular select. Brush 1–16. Filled or outline shapes. | `editor.rs`, `draw.rs` |
| Selection | Drag to move, arrow nudge, in-memory copy and paste, clear, select all | `editor.rs` |
| Canvas | Resize and crop. Mirror horizontal and vertical while stamping | `document.rs`, `editor.rs` |
| Layers | Add, delete, duplicate, reorder, rename, visibility, opacity, merge down. At most 32 | `document.rs` |
| Frames | Add, duplicate, duration 20–10000 ms, playback clock, onion skin in the view | `animation.rs`, `canvas.rs` |
| Undo | Up to 50 steps and 256 MiB, including the selection rectangle. Slider drags coalesce | `history.rs` |
| `.oxypix` | Version 1, little-endian, caps before allocation, trailing bytes rejected | `io/oxypix.rs`, `docs/FORMAT.md` |
| Raster | PNG, BMP, and GIF import and export. GIF import is one composited layer. Spritesheet and 2×/4× nearest PNG | `io/raster.rs` |
| CLI | `export-png`, `export-bmp`, `export-gif`, `export-sheet`. Failures print and exit. Missing file must not hang | `cli.rs`, `tests/cli.rs` |
| Window | egui 0.31 glow, nearest-neighbor canvas, grid, zoom, pan, shortcuts, in-code tool icons | `app.rs`, `canvas.rs` |
| Files UI | Recent list, drag and drop, Windows file dialog, Linux portal with a path field fallback | `recent.rs`, `native.rs` |
| Updates | Help, then Check for updates. Download only after `SHA256SUMS.txt` matches. No network until that command | `update.rs` |
| Windows package | Per-user NSIS installer and portable exe. Static CRT. Unsigned | `installer/oxypixel.nsi`, `release.yml` |
| Portable exe resources | The v0.3.2 release exe embeds an icon and VERSIONINFO (`FileVersion` 0.3.2). A cross-build without `rc.exe`, `llvm-rc`, or `windres` does not | `build.rs`, [README.md](../README.md), [BUILD-WINDOWS.md](BUILD-WINDOWS.md) |
| Linux package | `oxypixel-<version>-linux-x86_64.tar.gz` and `oxypixel-<version>-linux-x86_64.deb` | `scripts/package-linux.sh`, `scripts/package-deb.sh`, `release.yml` |
| CI | One Windows job per pull request. `push` is `main` only; tags stay on `release.yml`. Docs-only changes skip the matrix. Ubuntu also runs `make check-windows`. `ubuntu-latest` and `windows-latest`, fmt, clippy `-D warnings`, tests, release build. Job timeout 15 minutes | `.github/workflows/ci.yml`, `Makefile` |

### Partial

| Area | What is true now | Gap |
| --- | --- | --- |
| Clipboard | Copy and paste use an in-memory patch inside the editor | Not the system clipboard. See [OX-P1](#ox-p1) |
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

**Lock update filenames to `PackageKind`.** Status: open. Area: `crates/oxypixel/src/update.rs`.

Check for updates looks up `PackageKind::asset_name` and a file named `SHA256SUMS.txt`. The checksum test uses handwritten 0.3.2 names. Those strings can drift from `asset_name`.

Acceptance: a unit test builds the three names with `asset_name` for version 0.3.2 and checks that `expected_hash` finds each of them in a sums list. The names stay `oxypixel-0.3.2-windows-x86_64.exe`, `OxyPixel-0.3.2-Setup.exe`, and `oxypixel-0.3.2-linux-x86_64.tar.gz`.

### OX-012

**Test the recent-files directory.** Status: open. Area: `crates/oxypixel/src/recent.rs`.

`push` is tested. `data_dir` is not. On Windows it is `%APPDATA%\OxyPixel`. Elsewhere it is `$XDG_DATA_HOME/oxypixel` when that value is set, otherwise `~/.local/share/oxypixel`.

Acceptance: a unit test sets the relevant environment variable, calls the directory helper, and checks the last path component. It restores the variable before returning. No dialog and no disk write outside a temp dir.

## Proposals

Not scheduled. Start one only if a human asks.

### OX-P1

**System clipboard for the selection.** The editor clipboard is a private pixel patch so paste works with no OS clipboard crate. A follow-up could copy that patch to the system clipboard and still round-trip inside `oxypixel-core` without one.

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

## Human only

### OX-H1

**Authenticode signing and cutting a release.** Needs a certificate and a version decision. Agents do not push `v*` tags or run the release workflow to try this. The steps that already exist are in [docs/BUILD-WINDOWS.md](BUILD-WINDOWS.md) and `.github/workflows/release.yml`.
