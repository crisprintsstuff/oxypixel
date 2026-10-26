# AGENTS.md

OxyPixel is a pixel-art editor for Windows and Linux. Drawing, layers, frames, undo, and file IO live in `oxypixel-core`, which has no windowing code. The `oxypixel` binary is an egui window plus a small headless export CLI. The product name in prose is **OxyPixel**. Crate names, the binary, and paths stay lowercase.

This file is the guide for people and coding agents working in this repository. Read it before editing. The task list is [docs/ROADMAP.md](docs/ROADMAP.md). The document model is [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

OxyPixel is not a Photoshop-style command shell. There is no command registry, control socket, or MCP server. The editing seam is `oxypixel_core::Editor`. The window calls those methods. `cli::dispatch` handles export and exits. Do not add a second implementation of a tool in the GUI.

## Orientation

| Read | Why |
| --- | --- |
| [docs/ROADMAP.md](docs/ROADMAP.md) | What is done, partial, or missing, and the next task ids |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Document, tools, files, and the GUI seam |
| [docs/FORMAT.md](docs/FORMAT.md) | `.oxypix` version 1 and the size caps |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Tests, pull requests, version bumps |
| [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md) | Windows exe, NSIS installer, signing notes |
| [SECURITY.md](SECURITY.md) | Parse limits and the one network call |

## Workspace

```text
crates/oxypixel-core     document, tools, history, import/export. No GUI.
crates/oxypixel          egui window, shortcuts, recent files, update check, CLI
installer/               NSIS script and wizard art
scripts/                 Windows cross-build, installer, Linux tar.gz and .deb
.github/workflows/       CI (ci.yml) and release (release.yml)
```

`oxypixel` depends on `oxypixel-core`. `oxypixel-core` depends only on `image` and `thiserror`. It must not gain `egui`, `eframe`, `winit`, or `rfd`.

Platform UI dependencies stay on the binary:

- Windows: `rfd` with default features off (system file dialog; error boxes use `MessageBoxW`).
- Linux: `eframe` with the `x11` feature, and `rfd` through the XDG desktop portal.
- macOS is not a target. Do not add a macOS backend unless [docs/ROADMAP.md](docs/ROADMAP.md) grows a task for it.

Both crates set `unsafe_code = "forbid"`.

## Hard rules

**Bad input returns `oxypixel_core::Error`.** A truncated file, a hostile size, a missing path, or a drag from `i32::MIN` must not panic. Caps in `limits.rs` are checked before large allocations. Do not add `unwrap`, `expect`, `panic!`, `todo!`, or `unreachable!` on a path that sees a file, a pointer coordinate, or a network response. Tests may unwrap. `Editor`'s default of a 64×64 canvas and the package-version parse may use `expect` because those inputs are literals in this repo.

**Headless paths never open a modal dialog.** `cli::fail` prints to stderr and returns exit code 1. On Windows, `rfd` error boxes call `MessageBoxW` and block until a click. A headless runner never clicks, so `cargo test` waits until the job timeout. `native::alert` is only for when the editor window itself cannot start. Export commands and unit tests must not call it. The CI launch check sets `OXYPIXEL_NO_DIALOG=1`, which keeps that error on stderr instead of a box.

**Every bug fix gets a regression test.** Prefer `oxypixel-core` unit tests for editing and file behavior. CLI failures belong in `crates/oxypixel/tests/cli.rs`. The missing-file export test must keep a deadline and kill a hung process.

**Windows and Linux both matter.** CI runs `ubuntu-latest` and `windows-latest`. A change that is `#[cfg(windows)]` is invisible to Linux clippy. When the diff touches `cfg(windows)`, `cfg!(windows)`, or a Windows-only module, run `make check-windows` before pushing. That runs `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. It needs the MinGW compiler `x86_64-w64-mingw32-gcc` (Debian/Ubuntu: `gcc-mingw-w64-x86-64`) and `rustup target add x86_64-pc-windows-gnu`. If either is missing, the target prints the install command and exits 1. The MSVC target is what the Windows CI job and the release build use. `cargo test` on Linux does not execute the Windows-only branches. The Ubuntu CI job runs `make check-windows` as well.

**Clean-room.** Do not copy code, artwork, palettes, icons, or wording from Pixelorama, Aseprite, or other editors. Behavior ideas such as a pencil or a layer stack are fine. Implementations are written in this repository.

**Assets and licenses.** The program is MIT. See [LICENSE](LICENSE). The Ink palette, the application icon, the tool icons (drawn in code), and the `.oxypix` layout were made for this project. Installer bitmaps are project art. Direct crate licenses are listed in [docs/THIRD-PARTY.md](docs/THIRD-PARTY.md). A new third-party image, font, or snippet needs its license noted in that file in the same change. NSIS is a build tool only. It is not linked into `oxypixel.exe`.

**Version trio.** A version bump updates `Cargo.toml`, `installer/oxypixel.nsi` (`!define VERSION`, `VIProductVersion`, `VIFileVersion`), and `crates/oxypixel/assets/oxypixel.rc` together. The README links to `releases/latest` and names download files as `oxypixel-<version>-...`. Do not pin a release number in those links. A command that needs a real filename, such as `tar` or `apt install`, uses the version being released.

**Actions minutes.** The owner is short on GitHub Actions minutes, and Windows minutes count double. CI runs on pull requests and on pushes to `main`. It does not also run for the pull-request branch push, and it does not run on tags. A docs-only change (`*.md`, `docs/**`) skips the test matrix. Do not push tags and do not dispatch `.github/workflows/release.yml` to test. Do not rerun a green workflow. One PR is enough.

## Per-task loop

1. **Orient.** Read this file and the task in [docs/ROADMAP.md](docs/ROADMAP.md). Read the modules that task names. If the task id is missing or already done, stop and say so.
2. **Acceptance test first.** Add the failing test that defines done. For a bug, the test is the regression: it fails on the old behavior and passes after the fix.
3. **Implement.** Editing rules go in `oxypixel-core`. The window calls the core. It does not reimplement a tool.
4. **Check.** From the repo root:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

   `make check` is the same three commands. When the change touches Windows-only code, also run `make check-windows`.
5. **UI.** If the change is visible, run `cargo run -p oxypixel` on a display and exercise the control. Say what you clicked. If no display is available, say that and rely on the core or CLI test. There is no screenshot harness in this repo yet.
6. **Record.** Update the task row in [docs/ROADMAP.md](docs/ROADMAP.md) and any user-facing doc the behavior changed (`README.md`, `docs/SHORTCUTS.md`, `docs/FORMAT.md`, `CHANGELOG.md` only when cutting a release).
7. **Commit.** The message names the task id, for example `OX-001: run CI once per pull request`.

## Definition of done

- The acceptance test, or the bug's regression test, is in the diff and passes.
- `make check` is green. Windows-only diffs also pass the MinGW clippy command.
- New failure paths return `Error` or a CLI exit code. They do not panic or open a dialog.
- `oxypixel-core` still has no GUI crates.
- Docs and the roadmap row match the code.
- The pull request says what changed and how it was tested.
- No tag was pushed and the release workflow was not dispatched.

A release is a human decision. Agents do not bump the version, tag, or upload installers unless the task says to.

## Parallel work

- One branch and one pull request per task. Do not stack unrelated tasks.
- One owner per area while a branch is open. If that area is already taken, pick another open id.

| Area | Paths |
| --- | --- |
| Document | `crates/oxypixel-core/src/document.rs`, `history.rs`, `limits.rs`, `error.rs`, `color.rs`, `animation.rs` |
| Tools | `crates/oxypixel-core/src/editor.rs`, `draw.rs`, `selection.rs` |
| Files | `crates/oxypixel-core/src/io/` |
| Window | `crates/oxypixel/src/app.rs`, `canvas.rs`, `theme.rs`, `toolicons.rs`, `shortcuts.rs`, `icon.rs` |
| Platform | `crates/oxypixel/src/native.rs`, `recent.rs`, `cli.rs`, `update.rs`, `build.rs`, `crates/oxypixel/tests/` |
| Packer | `crates/oxypack/`, `crates/oxypixel/src/packview.rs` |
| Release | `installer/`, `scripts/`, `.github/workflows/release.yml` |
| Docs | `docs/`, `AGENTS.md`, `README.md`, `CONTRIBUTING.md` |

`app.rs` and `editor.rs` are the files that collide. Keep edits there small and re-read them before saving. New drawing behavior belongs in `draw.rs` or a tested `Editor` method, not a new branch of `app.rs`.

Use a private target directory when several agents build at once: `CARGO_TARGET_DIR=target/agent-$USER cargo test --workspace`. Delete it when the branch is done.

Decide anything the roadmap and these rules already cover. Ask a human before adding a dependency, changing the `.oxypix` format version, signing a binary, pushing a tag, or force-pushing.
