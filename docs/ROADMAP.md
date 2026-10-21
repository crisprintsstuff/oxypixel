# Roadmap and scorecard

Status date: 2026-10-21. Version in tree: 0.3.2 (`Cargo.toml`). GitHub issues: none open. This file is the work list for [AGENTS.md](../AGENTS.md).

Rows are taken from the code and the docs. **Proposal** means the item is a reasonable next step, not a commitment and not an open issue. Do not start a proposal unless a human asks for it.

When you finish a task, change its status here in the same pull request.

## Current focus

Infrastructure, then small fixes that already have a place in the code.

1. [OX-002](#ox-002) — one command for the Windows-target clippy check.

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
| Linux package | `oxypixel-<version>-linux-x86_64.tar.gz` from `scripts/package-linux.sh` | `release.yml` |
| CI | One Windows job per pull request. `push` is `main` only; tags stay on `release.yml`. Docs-only changes skip the matrix. `ubuntu-latest` and `windows-latest`, fmt, clippy `-D warnings`, tests, release build. Job timeout 15 minutes | `.github/workflows/ci.yml` |

### Partial

| Area | What is true now | Gap |
| --- | --- | --- |
| Clipboard | Copy and paste use an in-memory patch inside the editor | Not the system clipboard. See [OX-P1](#ox-p1) |
| GIF import | Every frame becomes a cel on one layer named Imported | Source layers are not in the GIF. This matches [docs/FORMAT.md](FORMAT.md) |
| File dialogs | Windows system dialog. Linux XDG portal, then the path field | No macOS build |
| Linux packages | The release archive is built and uploaded | `.deb`, RPM, AppImage, and Flatpak are only described in [LINUX-PACKAGING.md](LINUX-PACKAGING.md). See [OX-P2](#ox-p2) |
| Code signing | `docs/BUILD-WINDOWS.md` tells a human how to run `signtool` | No certificate in the repo or in CI. See [OX-H1](#ox-h1) |
| GUI checks | Canvas math, shortcuts, and the CLI have tests | Nothing clicks the window. See [OX-P3](#ox-p3) |

### Missing

Nothing below is an open GitHub issue. The proposals are optional. The human-only row is not agent work.

## Tasks

### OX-001

**Stop the duplicate Windows CI run.** Status: done. Area: `.github/workflows/ci.yml`.

`push` runs only on `main`. Pull requests use `pull_request`, so a branch no longer starts a second workflow. A concurrency group cancels a run that a newer push on the same ref replaces. Tag pushes stay on `release.yml`. A change that only touches `*.md` or `docs/**` skips the Ubuntu and Windows jobs with a job condition, which reports success. `paths-ignore` was not used: a workflow skipped that way never reports, and a required check stays pending. The comment at the top of `ci.yml` says why. The pull request's own CI is the check. `release.yml` was not dispatched.

### OX-002

**One command for Windows-target clippy.** Status: open. Area: `Makefile`.

Linux `cargo clippy` does not compile `#[cfg(windows)]` code. Agents are supposed to run the MinGW target clippy from [AGENTS.md](../AGENTS.md) when they touch that code. There is no Make target for it.

Acceptance: `make check-windows` runs `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. The target's comment names the MinGW compiler it needs. A clean tree exits 0.

### OX-010

**Reconcile the portable-exe icon sentence.** Status: done. Area: `README.md`, `docs/BUILD-WINDOWS.md`.

The v0.3.2 portable exe on the releases page (`oxypixel-0.3.2-windows-x86_64.exe`) contains an ICON, a GROUP_ICON, and a VERSION resource. `FileVersion` and `ProductVersion` are 0.3.2. README and `docs/BUILD-WINDOWS.md` now say the same thing: that tagged Windows build embeds the icon and version resource, and a cross-build without `rc.exe`, `llvm-rc`, or `windres` does not. No new release was cut.

### OX-011

**Lock update filenames to `PackageKind`.** Status: done. Area: `crates/oxypixel/src/update.rs`.

`release_checksum_list_names_each_package` builds the three 0.3.2 names with `PackageKind::asset_name` and checks that `expected_hash` finds each of them. The names stay `oxypixel-0.3.2-windows-x86_64.exe`, `OxyPixel-0.3.2-Setup.exe`, and `oxypixel-0.3.2-linux-x86_64.tar.gz`.

### OX-012

**Test the recent-files directory.** Status: done. Area: `crates/oxypixel/src/recent.rs`.

`data_dir_uses_the_platform_folder` sets `APPDATA` on Windows and `XDG_DATA_HOME` elsewhere, calls `data_dir`, and checks that the last path component is `OxyPixel` or `oxypixel`. The variable is restored when the test returns. The test does not open a dialog or write a file.

## Proposals

Not scheduled. Start one only if a human asks.

### OX-P1

**System clipboard for the selection.** The editor clipboard is a private pixel patch so paste works with no OS clipboard crate. A follow-up could copy that patch to the system clipboard and still round-trip inside `oxypixel-core` without one.

### OX-P2

**A `.deb` as written in LINUX-PACKAGING.md.** `cargo-deb` and `nfpm` are not configured. The release artifact remains the tar.gz until this is chosen.

### OX-P3

**A window check that does not need a new control protocol.** CONTRIBUTING says to run the app after a UI change. A later task could render the built-in example through the existing composite and compare a hash. It should not add a command server.

## Human only

### OX-H1

**Authenticode signing and cutting a release.** Needs a certificate and a version decision. Agents do not push `v*` tags or run the release workflow to try this. The steps that already exist are in [docs/BUILD-WINDOWS.md](BUILD-WINDOWS.md) and `.github/workflows/release.yml`.
