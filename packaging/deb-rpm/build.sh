#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="${DIST:-$ROOT/dist/packages}"
OUT_LINK="${OUT_LINK:-$ROOT/result-all-dist}"

mkdir -p "$DIST"
nix build "$ROOT#AllDist" --out-link "$OUT_LINK" "$@"
cp -f "$OUT_LINK"/deb/*.deb "$OUT_LINK"/rpm/*.rpm "$DIST"/
