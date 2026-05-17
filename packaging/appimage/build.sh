#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APPDIR="${APPDIR:-$ROOT/AppDir}"
DIST="${DIST:-$ROOT/dist}"
APPIMAGE_NAME="${APPIMAGE_NAME:-Hearthstone_Linux-x86_64.AppImage}"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

copy_lib_by_name() {
  local name="$1"
  local target="$APPDIR/usr/lib/hearthstone-linux-runtime"
  local path
  path="$(ldconfig -p 2>/dev/null | awk -v name="$name" '$1 == name { print $NF; exit }')"
  if [ -n "$path" ] && [ -e "$path" ]; then
    install -Dm755 "$path" "$target/$(basename "$path")"
  fi
}

require cargo
require linuxdeploy
require appimagetool
require patchelf

if [ "$(uname -m)" != "x86_64" ]; then
  echo "AppImage packaging currently supports x86_64 only" >&2
  exit 1
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/metainfo" \
  "$APPDIR/usr/share/hearthstone-linux/stubs" \
  "$APPDIR/usr/share/icons/hicolor/scalable/apps" \
  "$APPDIR/usr/lib/hearthstone-linux-runtime" \
  "$DIST"

cargo build --release --workspace

install -Dm755 "$ROOT/target/release/hearthstone-linux" "$APPDIR/usr/bin/hearthstone-linux"
install -Dm755 "$ROOT/target/release/libCoreFoundation.so" "$APPDIR/usr/share/hearthstone-linux/stubs/CoreFoundation.so"
install -Dm755 "$ROOT/target/release/libOSXWindowManagement.so" "$APPDIR/usr/share/hearthstone-linux/stubs/libOSXWindowManagement.so"
install -Dm755 "$ROOT/target/release/libblz_commerce_sdk_plugin.so" "$APPDIR/usr/share/hearthstone-linux/stubs/libblz_commerce_sdk_plugin.so"
install -Dm644 "$ROOT/assets/client.config.in" "$APPDIR/usr/share/hearthstone-linux/client.config.in"
install -Dm644 "$ROOT/data/io.github.hearthstone_linux.desktop" "$APPDIR/usr/share/applications/io.github.hearthstone_linux.desktop"
install -Dm644 "$ROOT/data/io.github.hearthstone_linux.metainfo.xml" "$APPDIR/usr/share/metainfo/io.github.hearthstone_linux.metainfo.xml"
install -Dm644 "$ROOT/packaging/appimage/io.github.hearthstone_linux.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux.svg"
install -Dm755 "$(command -v patchelf)" "$APPDIR/usr/bin/patchelf"
install -Dm755 "$ROOT/packaging/appimage/AppRun" "$APPDIR/AppRun"

ln -sf usr/share/applications/io.github.hearthstone_linux.desktop "$APPDIR/io.github.hearthstone_linux.desktop"
ln -sf usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux.svg "$APPDIR/io.github.hearthstone_linux.svg"

if [ -e /lib64/ld-linux-x86-64.so.2 ]; then
  install -Dm755 /lib64/ld-linux-x86-64.so.2 "$APPDIR/usr/lib/hearthstone-linux-runtime/ld-linux-x86-64.so.2"
fi

copy_lib_by_name libc.so.6
copy_lib_by_name libm.so.6
copy_lib_by_name libdl.so.2
copy_lib_by_name libpthread.so.0
copy_lib_by_name librt.so.1
copy_lib_by_name libgcc_s.so.1
copy_lib_by_name libstdc++.so.6

linuxdeploy \
  --appdir="$APPDIR" \
  --executable="$APPDIR/usr/bin/hearthstone-linux" \
  --desktop-file="$APPDIR/usr/share/applications/io.github.hearthstone_linux.desktop" \
  --icon-file="$APPDIR/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux.svg" \
  --custom-apprun="$ROOT/packaging/appimage/AppRun"

ARCH=x86_64 appimagetool "$APPDIR" "$DIST/$APPIMAGE_NAME"
