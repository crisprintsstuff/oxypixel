#!/usr/bin/env bash
# Pack OxyPixel-<version>-macos-universal.dmg on a Mac.
#
# Builds oxypixel and oxypack for Apple Silicon (aarch64) and Intel
# (x86_64), joins each pair into one universal binary with lipo, and puts
# them in OxyPixel.app with docs/macos/Info.plist and an icon made from
# docs/linux/oxypixel-256.png. The app is signed ad hoc (codesign -s -), which
# Apple Silicon needs to run it at all; it is not signed with a Developer ID
# or notarized, so the first launch needs right-click, Open. Only tools that
# ship with macOS and Xcode's command line tools are used: lipo, sips,
# iconutil, codesign, plutil, and hdiutil.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if [[ "$(uname -s)" != Darwin ]]; then
  echo "package-macos.sh runs on macOS." >&2
  exit 1
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Could not read a major.minor.patch version from Cargo.toml" >&2
  exit 1
fi

export MACOSX_DEPLOYMENT_TARGET=11.0
targets=(aarch64-apple-darwin x86_64-apple-darwin)
for target in "${targets[@]}"; do
  cargo build --release --target "$target" -p oxypixel -p oxypack
done

if [[ -n "${CARGO_TARGET_DIR:-}" && "${CARGO_TARGET_DIR}" != /* ]]; then
  CARGO_TARGET_DIR="$root/$CARGO_TARGET_DIR"
fi
target_dir="${CARGO_TARGET_DIR:-$root/target}"

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
app="$stage/dmg/OxyPixel.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

for bin in oxypixel oxypack; do
  lipo -create -output "$app/Contents/MacOS/$bin" \
    "$target_dir/aarch64-apple-darwin/release/$bin" \
    "$target_dir/x86_64-apple-darwin/release/$bin"
  archs=$(lipo -archs "$app/Contents/MacOS/$bin")
  if [[ " $archs " != *" arm64 "* || " $archs " != *" x86_64 "* ]]; then
    echo "$bin has architectures '$archs', expected arm64 and x86_64" >&2
    exit 1
  fi
done

sed "s/@VERSION@/$version/g" docs/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
cp LICENSE "$app/Contents/Resources/LICENSE.txt"

# The icon: each size the iconset wants, sampled from the 256-pixel PNG.
iconset="$stage/OxyPixel.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256; do
  sips -z "$size" "$size" docs/linux/oxypixel-256.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  if [[ "$double" -le 256 ]]; then
    sips -z "$double" "$double" docs/linux/oxypixel-256.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
  fi
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/OxyPixel.icns"

# Sign the inner tool first, then the bundle around it.
codesign --force --sign - --timestamp=none "$app/Contents/MacOS/oxypack"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict --verbose=1 "$app"

reported=$("$app/Contents/MacOS/oxypixel" --version)
if [[ "$reported" != "oxypixel ${version}" ]]; then
  echo "the app reported '${reported}', expected 'oxypixel ${version}'" >&2
  exit 1
fi

# The disk image: the app and a link to Applications to drag it onto.
ln -s /Applications "$stage/dmg/Applications"
mkdir -p dist
out="$root/dist/OxyPixel-${version}-macos-universal.dmg"
rm -f "$out"
hdiutil create -volname "OxyPixel ${version}" -srcfolder "$stage/dmg" -ov -format UDZO "$out" >/dev/null
hdiutil verify "$out" >/dev/null

echo "Wrote ${out}"
