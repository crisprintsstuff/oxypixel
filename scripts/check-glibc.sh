#!/usr/bin/env bash
# Fail when a binary needs a newer glibc than the oldest system we support.
# Usage: scripts/check-glibc.sh <binary>...
# OXYPIXEL_MAX_GLIBC overrides the limit (default 2.39, Ubuntu 24.04).
set -euo pipefail

max="${OXYPIXEL_MAX_GLIBC:-2.39}"
if ! command -v objdump >/dev/null 2>&1; then
  echo "objdump is required to check glibc versions (package binutils)." >&2
  exit 1
fi
status=0
for bin in "$@"; do
  need=$(objdump -T "$bin" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
  if [[ -z "$need" ]]; then
    echo "$bin: no glibc symbol versions found" >&2
    status=1
    continue
  fi
  highest=$(printf '%s\n%s\n' "$need" "$max" | sort -V | tail -1)
  if [[ "$highest" != "$max" ]]; then
    echo "$bin needs glibc $need, newer than $max. Build on an older system (the release builds on Ubuntu 24.04)." >&2
    status=1
  else
    echo "$bin needs glibc $need (limit $max)"
  fi
done
exit "$status"
