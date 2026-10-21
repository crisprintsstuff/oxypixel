# Roadmap and scorecard

Status date: 2026-10-21. Version in tree: 0.3.2 (`Cargo.toml`). GitHub issues: none open. This file is the work list for [AGENTS.md](../AGENTS.md).

Rows are taken from the code and the docs. **Proposal** means the item is a reasonable next step, not a commitment and not an open issue. Do not start a proposal unless a human asks for it.

When you finish a task, change its status here in the same pull request.

## Current focus

Infrastructure, then small fixes that already have a place in the code.

1. [OX-001](#ox-001) — stop paying for two Windows CI runs on a pull request.
2. [OX-002](#ox-002) — one command for the Windows-target clippy check.
3. [OX-010](#ox-010) — make the portable-exe icon sentence match the release build.
4. [OX-011](#ox-011) — test that update filenames stay tied to `PackageKind`.
5. [OX-012](#ox-012) — test the recent-files directory on both platforms.

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
| Linux package | `oxypixel-<version>-linux-x86_64.tar.gz` from `scripts/package-linux.sh` | `release.yml` |
| CI | `ubuntu-latest` and `windows-latest`, fmt, clippy `-D warnings`, tests, release build. Job timeout 15 minutes | `.github/workflows/ci.yml` |

### Partial

| Area | What is true now | Gap |
| --- | --- | --- |
| Clipboard | Copy and paste use an in-memory patch inside the editor | Not the system clipboard. See [OX-P1](#ox-p1) |
| GIF import | Every frame becomes a cel on one layer named Imported | Source layers are not in the GIF. This matches [docs/FORMAT.md](FORMAT.md) |
| File dialogs | Windows system dialog. Linux XDG portal, then the path field | No macOS build |
| Linux packages | The release archive is built and uploaded | `.deb`, RPM, AppImage, and Flatpak are only described in [LINUX-PACKAGING.md](LINUX-PACKAGING.md). See [OX-P2](#ox-p2) |
| Code signing | `docs/BUILD-WINDOWS.md` tells a human how to run `signtool` | No certificate in the repo or in CI. See [OX-H1](#ox-h1) |
| Portable exe resources | The Windows release job puts `rc.exe` on `PATH` when the SDK has it, so a tagged build can embed an icon and `VERSIONINFO` | A Linux cross-build without a resource compiler does not. [README.md](../README.md) still says the portable copy has no embedded icon. See [OX-010](#ox-010) |
| GUI checks | Canvas math, shortcuts, and the CLI have tests | Nothing clicks the window. See [OX-P3](#ox-p3) |
| CI cost | Both OS jobs are required | A branch push and the pull request each start a Windows job. See [OX-001](#ox-001) |

### Missing

Nothing below is an open GitHub issue. The proposals are optional. The human-only row is not agent work.

## Tasks

### OX-001

**Stop the duplicate Windows CI run.** Status: open. Area: `.github/workflows/ci.yml`.

`on: push` and `on: pull_request` both run the full matrix. A pull-request branch therefore starts two Windows jobs. Windows minutes are the scarce ones.

Acceptance: a commit on a pull-request branch starts one Windows job, a push to `main` still runs CI, and a short comment in the workflow says why. Do not dispatch the release workflow to prove it. The pull request's own CI is the check.

### OX-002

**One command for Windows-target clippy.** Status: open. Area: `Makefile`.

Linux `cargo clippy` does not compile `#[cfg(windows)]` code. Agents are supposed to run the MinGW target clippy from [AGENTS.md](../AGENTS.md) when they touch that code. There is no Make target for it.

Acceptance: `make check-windows` runs `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. The target's comment names the MinGW compiler it needs. A clean tree exits 0.

### OX-010

**Reconcile the portable-exe icon sentence.** Status: open. Area: `README.md`, `docs/BUILD-WINDOWS.md`.

The README says the portable Windows executable has no embedded icon or version resource. `docs/BUILD-WINDOWS.md` says the GitHub Actions job embeds them when `rc.exe` is on `PATH`. Both cannot describe the same 0.3.2 release file.

Acceptance: both docs describe the same thing. If the release exe from the Actions job embeds the resource, the README says so and still says a cross-build without `rc.exe` does not. No new release is cut to find out. Use the existing 0.3.2 asset or the release-job log.

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

**A `.deb` as written in LINUX-PACKAGING.md.** `cargo-deb` and `nfpm` are not configured. The release artifact remains the tar.gz until this is chosen.

### OX-P3

**A window check that does not need a new control protocol.** CONTRIBUTING says to run the app after a UI change. A later task could render the built-in example through the existing composite and compare a hash. It should not add a command server.

## Human only

### OX-H1

**Authenticode signing and cutting a release.** Needs a certificate and a version decision. Agents do not push `v*` tags or run the release workflow to try this. The steps that already exist are in [docs/BUILD-WINDOWS.md](BUILD-WINDOWS.md) and `.github/workflows/release.yml`.
