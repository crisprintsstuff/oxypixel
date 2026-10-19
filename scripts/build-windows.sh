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

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
mkdir -p dist
cp -f target/x86_64-pc-windows-msvc/release/oxypixel.exe dist/oxypixel.exe
cp -f dist/oxypixel.exe "dist/oxypixel-${version}-windows-x86_64.exe"
cp -f installer/oxypixel.ico dist/oxypixel.ico
echo "Wrote dist/oxypixel-${version}-windows-x86_64.exe (unsigned, Authenticode-ready)"
