#!/usr/bin/env bash
# Pack the Linux release binary. This is an x86_64 executable, not a .deb or AppImage.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
name="oxypixel-${version}-linux-x86_64"

cargo build --release -p oxypixel

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name"
cp -f target/release/oxypixel "$stage/$name/oxypixel"
cp -f LICENSE "$stage/$name/LICENSE"
cat > "$stage/$name/README.txt" << EOF
OxyPixel ${version} for Linux x86_64

Run ./oxypixel from this directory. The binary was built on Ubuntu 24.04.
Opening the window needs an X11 display and these runtime libraries:

  libGL.so.1
  libX11.so.6
  libxkbcommon.so.0

Headless export does not need a display:

  ./oxypixel export-png drawing.oxypix frame.png

This archive is the program only. It is not a .deb, RPM, AppImage, or Flatpak.
EOF
chmod 755 "$stage/$name/oxypixel"
mkdir -p dist
tar -C "$stage" -czf "dist/${name}.tar.gz" "$name"
echo "Wrote dist/${name}.tar.gz"
