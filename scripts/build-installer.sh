#!/usr/bin/env bash
# Build the NSIS installer in a container so the host does not need root.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if [[ ! -f dist/oxypixel.exe ]]; then
  echo "dist/oxypixel.exe is missing. Run scripts/build-windows.sh first." >&2
  exit 1
fi
if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required to run makensis on Linux." >&2
  exit 1
fi

docker run --rm \
  -v "$root":/work \
  -w /work \
  ubuntu:24.04 \
  bash -lc 'apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y nsis && makensis installer/oxypixel.nsi'

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd dist
    sha256sum oxypixel.exe oxypixel.ico OxyPixel-0.1.0-Setup.exe > SHA256SUMS.txt
  )
fi
echo "Wrote dist/OxyPixel-0.1.0-Setup.exe (unsigned)"
