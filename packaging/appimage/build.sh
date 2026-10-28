#!/usr/bin/env bash
# Builds release artifacts into dist/ (packaging/linux/Dockerfile).
# Native packages use the AlmaLinux 9 stage. The AppImage uses the AlmaLinux 8
# GTK toolchain stage in the same Dockerfile. Works with docker or podman.
#
#   EL8_GTK_IMAGE=ghcr.io/<owner>/<repo>/el8-gtk:<tag> packaging/appimage/build.sh
# reuses a published toolchain image instead of compiling GTK.
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
build_args=()
if [ -n "${EL8_GTK_IMAGE:-}" ]; then
  build_args+=(--build-arg "EL8_GTK_IMAGE=${EL8_GTK_IMAGE}")
fi
"$ENGINE" build -f "$ROOT/packaging/linux/Dockerfile" --target "$TARGET" \
  --output "type=local,dest=$DIST" "${build_args[@]}" "$@" "$ROOT"
ls -l "$DIST"
