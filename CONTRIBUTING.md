# Contributing to OxyPixel

OxyPixel is an original project. Do not copy code, artwork, palettes, icons, or wording from Pixelorama or other pixel editors. Ideas such as "a pencil tool" or "layers" are fine. Implementations here should be written for this repository.

## Setup

- Rust 1.99.0, which `rust-toolchain.toml` selects, including `rustfmt` and `clippy`
- For a Windows installer, see [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`make check` runs the same three commands. `make check-windows` runs clippy with warnings denied for the `x86_64-pc-windows-gnu` target. It needs `x86_64-w64-mingw32-gcc` and that rustup target. [AGENTS.md](AGENTS.md) says when to run it.

## What to change

- Put editing rules in `crates/oxypixel-core`. If a behavior matters, add a unit test there. The GUI should call the core instead of reimplementing it.
- Keep `unsafe` out of this repository. The crate lints forbid it.
- New file-format fields need a version bump in `FORMAT_VERSION` and a note in [docs/FORMAT.md](docs/FORMAT.md).
- Keep user-facing errors as `oxypixel_core::Error` values with a sentence that says what failed. Do not panic on bad input.
- The Windows installer version is duplicated in `installer/oxypixel.nsi` (`!define VERSION`) and the workspace `Cargo.toml` version. Update both together, and the `VERSIONINFO` in `crates/oxypixel/assets/oxypixel.rc`.
- README download links stay on the latest release. Describe files as `oxypixel-<version>-...`. Example commands that need a real filename use the version being released.

## Pull requests

Use a focused branch. Describe the behavior change and how you tested it. Run the checks above before asking for review. Coding agents follow [AGENTS.md](AGENTS.md) and take the next open id from [docs/ROADMAP.md](docs/ROADMAP.md).

Do not commit signing keys, `.pfx` files, tokens, or built binaries. `dist/` is gitignored on purpose. Release artifacts come from the Windows release workflow or from `scripts/build-windows.sh`.

## Tests that matter

- Tool geometry, selection clipping, undo, and layer/frame edits
- `.oxypix` roundtrip, truncated files, and hostile sizes that must fail before a large allocation
- PNG, BMP, and GIF import/export
- `oxypixel --help` and `oxypixel export-png` through `crates/oxypixel/tests/cli.rs`
- The built-in example, composited with `Document::composite` and compared to the hash and golden PNGs in `crates/oxypixel-core/tests/golden/`. This runs in `cargo test` with no window and no dialog. After an intentional change to that picture, refresh the fixtures, review the PNG diff, run the test again with the variable unset, and commit the golden directory:

  ```bash
  OXYPIXEL_UPDATE_DEMO_GOLDEN=1 cargo test -p oxypixel-core --test demo_composite -- --nocapture
  ```

Nothing clicks the window. After a UI change, run the app and try pencil, undo, save, and export if a display is available. The composite check above covers the pixels the canvas uploads for the built-in example when its paper layer is opaque.

## Conduct

Be direct and respectful in reviews. Critique the change, not the person.
