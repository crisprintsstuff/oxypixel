#!/usr/bin/env bash
# Cross-compile the unsigned Windows executable from Linux with cargo-xwin.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if ! cargo xwin --version >/dev/null 2>&1; then
  echo "cargo-xwin is required. Install it with: cargo install cargo-xwin" >&2
  exit 1
fi
if ! command -v clang-cl >/dev/null 2>&1; then
  echo "clang-cl must be on PATH (LLVM or a Swift clang toolchain)." >&2
  exit 1
fi

cargo xwin build --release --target x86_64-pc-windows-msvc -p oxypixel --cross-compiler clang-cl

mkdir -p dist
cp -f target/x86_64-pc-windows-msvc/release/oxypixel.exe dist/oxypixel.exe
cp -f installer/oxypixel.ico dist/oxypixel.ico
if command -v sha256sum >/dev/null 2>&1; then
  (
    cd dist
    sha256sum oxypixel.exe oxypixel.ico > SHA256SUMS.txt
  )
fi
echo "Wrote dist/oxypixel.exe (unsigned, Authenticode-ready)"
