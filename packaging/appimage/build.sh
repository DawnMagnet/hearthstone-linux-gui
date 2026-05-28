#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="${DIST:-$ROOT/dist}"
BUILD_OUT="${BUILD_OUT:-$ROOT/dist/docker-build}"

rm -rf "$BUILD_OUT"
mkdir -p "$BUILD_OUT" "$DIST"
docker build "$@" \
  --target dist \
  --file "$ROOT/packaging/native/Dockerfile" \
  --output "type=local,dest=$BUILD_OUT" \
  "$ROOT"
cp -f "$BUILD_OUT"/hearthstone-linux-gui-*-x86_64.AppImage "$DIST"/
