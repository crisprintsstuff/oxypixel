#!/usr/bin/env bash
# Pack oxypixel-<version>-linux-x86_64.deb with cargo-deb.
# cargo-deb is a build tool only. It is not linked into the editor.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

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

if ! command -v dpkg-shlibdeps >/dev/null 2>&1; then
  echo "dpkg-shlibdeps is required so cargo-deb can record linked libraries (package dpkg-dev)." >&2
  exit 1
fi

cargo_deb_version="3.8.0"
cargo_deb_sha256="c6bd0a4affaa232814cc615b2597457f1d44fdfe1c8ebd643f3a491a1f244040"
cache="$target_dir/cargo-deb-${cargo_deb_version}"
tool="$cache/usr/bin/cargo-deb"
if [[ ! -x "$tool" ]]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  deb="$tmp/cargo-deb_${cargo_deb_version}-1_amd64.deb"
  url="https://github.com/kornelski/cargo-deb/releases/download/v${cargo_deb_version}/cargo-deb_${cargo_deb_version}-1_amd64.deb"
  curl -fsSL -o "$deb" "$url"
  echo "${cargo_deb_sha256}  ${deb}" | sha256sum -c -
  mkdir -p "$cache"
  dpkg-deb -x "$deb" "$cache"
  rm -rf "$tmp"
  trap - EXIT
fi

mkdir -p dist
out="$root/dist/oxypixel-${version}-linux-x86_64.deb"
"$tool" -p oxypixel --no-build --output "$out"

control=$(dpkg-deb -f "$out")
package=$(printf '%s\n' "$control" | sed -n 's/^Package: //p')
debver=$(printf '%s\n' "$control" | sed -n 's/^Version: //p')
arch=$(printf '%s\n' "$control" | sed -n 's/^Architecture: //p')
depends=$(printf '%s\n' "$control" | sed -n 's/^Depends: //p')
if [[ "$package" != oxypixel || "$debver" != "${version}-1" || "$arch" != amd64 ]]; then
  echo "unexpected control fields:" >&2
  printf '%s\n' "$control" >&2
  exit 1
fi
# Match a package token. `libxkbcommon0` is a prefix of `libxkbcommon-x11-0`,
# so a substring search would pass when only the longer name is present.
dep_list=", ${depends},"
for pkg in libgl1 libx11-6 libxkbcommon0 libxcb1 libxkbcommon-x11-0 libc6; do
  case "$dep_list" in
    *", ${pkg},"* | *", ${pkg} ("*) ;;
    *)
      echo "Depends is missing ${pkg}: ${depends}" >&2
      exit 1
      ;;
  esac
done

listing=$(dpkg-deb -c "$out")
for path in \
  ./usr/bin/oxypixel \
  ./usr/bin/oxypack \
  ./usr/share/applications/oxypixel.desktop \
  ./usr/share/icons/hicolor/32x32/apps/oxypixel.png \
  ./usr/share/mime/packages/oxypixel.xml \
  ./usr/share/doc/oxypixel/copyright
do
  if ! printf '%s\n' "$listing" | grep -q " ${path}$"; then
    echo "package is missing ${path}" >&2
    printf '%s\n' "$listing" >&2
    exit 1
  fi
done
for exe in oxypixel oxypack; do
  if ! printf '%s\n' "$listing" | grep -E "^-..x.* \./usr/bin/${exe}$" >/dev/null; then
    echo "/usr/bin/${exe} is not marked executable" >&2
    printf '%s\n' "$listing" >&2
    exit 1
  fi
done

echo "Wrote ${out}"
