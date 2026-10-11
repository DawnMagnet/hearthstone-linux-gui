#!/usr/bin/env bash
# Builds release artifacts into dist/ using the Enterprise Linux 9 baseline
# container (packaging/linux/Dockerfile). Works with docker or podman.
#
#   packaging/appimage/build.sh                  # AppImage + deb/rpm/pacman
#   TARGET=appimage-out packaging/appimage/build.sh   # AppImage only
#
# Extra arguments go to `build`, e.g.
#   packaging/appimage/build.sh --build-arg ALMALINUX_MIRROR=https://ftp.osuosl.org/pub/almalinux
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="${DIST:-$ROOT/dist}"
TARGET="${TARGET:-dist}"
ENGINE="${CONTAINER_ENGINE:-$(command -v docker || command -v podman)}"

mkdir -p "$DIST"
"$ENGINE" build -f "$ROOT/packaging/linux/Dockerfile" --target "$TARGET" \
  --output "type=local,dest=$DIST" "$@" "$ROOT"
ls -l "$DIST"
