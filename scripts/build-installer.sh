#!/usr/bin/env bash
# Build the NSIS installer in a container so the host does not need root.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

for exe in oxypixel.exe oxypack.exe; do
  if [[ ! -f "dist/${exe}" ]]; then
    echo "dist/${exe} is missing. Run scripts/build-windows.sh first." >&2
    exit 1
  fi
done
if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required to run makensis on Linux." >&2
  exit 1
fi

docker run --rm \
  -v "$root":/work \
  -w /work \
  ubuntu:24.04 \
  bash -lc 'apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y nsis && makensis installer/oxypixel.nsi'

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
echo "Wrote dist/OxyPixel-${version}-Setup.exe (unsigned)"
