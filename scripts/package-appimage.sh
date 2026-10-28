#!/usr/bin/env bash
# Pack OxyPixel-<version>-x86_64.AppImage from the release binaries.
#
# The AppDir is assembled here and packed with mksquashfs (package
# squashfs-tools). The AppImage runtime is the pinned type2-runtime release
# below, checked against its SHA-256, the same way package-deb.sh pins
# cargo-deb. No AppImage tool is run. Nothing is bundled beyond our two
# binaries: like the tar.gz, the AppImage loads libGL, libX11, and
# libxkbcommon from the system and needs glibc 2.39 or newer.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

app_id="io.github.crisprintsstuff.OxyPixel"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Could not read a major.minor.patch version from Cargo.toml" >&2
  exit 1
fi

python3 scripts/gen_icon.py --check

if [[ -n "${CARGO_TARGET_DIR:-}" && "${CARGO_TARGET_DIR}" != /* ]]; then
  CARGO_TARGET_DIR="$root/$CARGO_TARGET_DIR"
fi
target_dir="${CARGO_TARGET_DIR:-$root/target}"
export CARGO_TARGET_DIR="$target_dir"

if [[ ! -x "$target_dir/release/oxypixel" || ! -x "$target_dir/release/oxypack" ]]; then
  cargo build --release -p oxypixel -p oxypack
fi

for tool in mksquashfs unsquashfs; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required (package squashfs-tools)." >&2
    exit 1
  fi
done

runtime_version="20251108"
runtime_sha256="2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d"
runtime="$target_dir/appimage-runtime-${runtime_version}-x86_64"
if [[ ! -f "$runtime" ]]; then
  url="https://github.com/AppImage/type2-runtime/releases/download/${runtime_version}/runtime-x86_64"
  curl -fsSL -o "$runtime.part" "$url"
  echo "${runtime_sha256}  ${runtime}.part" | sha256sum -c -
  mv "$runtime.part" "$runtime"
fi
echo "${runtime_sha256}  ${runtime}" | sha256sum -c - >/dev/null

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
appdir="$stage/AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" \
  "$appdir/usr/share/icons/hicolor/256x256/apps" "$appdir/usr/share/metainfo" \
  "$appdir/usr/share/mime/packages" "$appdir/usr/share/doc/oxypixel"
install -m 755 "$target_dir/release/oxypixel" "$appdir/usr/bin/oxypixel"
install -m 755 "$target_dir/release/oxypack" "$appdir/usr/bin/oxypack"
install -m 644 docs/linux/oxypixel.desktop "$appdir/usr/share/applications/${app_id}.desktop"
install -m 644 docs/linux/oxypixel-256.png "$appdir/usr/share/icons/hicolor/256x256/apps/oxypixel.png"
install -m 644 "docs/linux/${app_id}.metainfo.xml" "$appdir/usr/share/metainfo/${app_id}.metainfo.xml"
install -m 644 docs/linux/oxypixel-mime.xml "$appdir/usr/share/mime/packages/oxypixel.xml"
install -m 644 LICENSE "$appdir/usr/share/doc/oxypixel/copyright"
# The AppImage spec reads the desktop file and icon from the AppDir root.
install -m 644 docs/linux/oxypixel.desktop "$appdir/${app_id}.desktop"
install -m 644 docs/linux/oxypixel-256.png "$appdir/oxypixel.png"
ln -s oxypixel.png "$appdir/.DirIcon"
cat > "$appdir/AppRun" << 'EOF'
#!/bin/sh
# Run the editor, or OxyPack when the AppImage is started through a link
# named oxypack.
here=$(dirname "$(readlink -f "$0")")
case "$(basename "${ARGV0:-$0}")" in
  oxypack*) exec "$here/usr/bin/oxypack" "$@" ;;
  *) exec "$here/usr/bin/oxypixel" "$@" ;;
esac
EOF
chmod 755 "$appdir/AppRun"

payload="$stage/payload.squashfs"
# Fixed times and root ownership keep the payload the same for the same input.
mksquashfs "$appdir" "$payload" -root-owned -noappend -no-xattrs -comp zstd \
  -mkfs-time 0 -all-time 0 -quiet >/dev/null

mkdir -p dist
out="$root/dist/OxyPixel-${version}-x86_64.AppImage"
cat "$runtime" "$payload" > "$out"
chmod 755 "$out"

# Read the payload back without running the runtime.
offset=$(stat -c %s "$runtime")
listing=$(unsquashfs -o "$offset" -l "$out")
for path in AppRun "${app_id}.desktop" oxypixel.png .DirIcon usr/bin/oxypixel usr/bin/oxypack \
  "usr/share/metainfo/${app_id}.metainfo.xml" usr/share/icons/hicolor/256x256/apps/oxypixel.png
do
  if ! printf '%s\n' "$listing" | grep -qx "squashfs-root/${path}"; then
    echo "AppImage is missing ${path}" >&2
    printf '%s\n' "$listing" >&2
    exit 1
  fi
done

# The release job sets this to start the AppImage once, without FUSE.
if [[ "${OXYPIXEL_APPIMAGE_SMOKE:-0}" == 1 ]]; then
  reported=$(APPIMAGE_EXTRACT_AND_RUN=1 "$out" --version)
  if [[ "$reported" != "oxypixel ${version}" ]]; then
    echo "the AppImage reported '${reported}', expected 'oxypixel ${version}'" >&2
    exit 1
  fi
fi

echo "Wrote ${out}"
