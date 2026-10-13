#!/usr/bin/env bash
# Installs the release build into an FHS tree (default <pkgroot>/usr/..., or
# <pkgroot>$PREFIX/... when PREFIX is set, e.g. PREFIX=/app for Flatpak).
# Shared by the deb/rpm/pacman packages and the AppImage.
set -euo pipefail

root="${1:?usage: stage.sh <pkgroot>}"
# PREFIX=/app is used by the Flatpak build; native packages keep /usr.
prefix="${PREFIX:-/usr}"
target="${CARGO_TARGET_DIR:-target}/release"
app_id=io.github.hearthstone_linux_gui
share="$root$prefix/share/hearthstone-linux-gui"
icon=data/icons/$app_id.svg

install -Dm755 "$target/hearthstone-linux-gui" "$root$prefix/bin/hearthstone-linux-gui"

# Stub libraries loaded into the Unity player by the host's dynamic loader.
install -Dm755 "$target/libCoreFoundation.so" "$share/stubs/CoreFoundation.so"
for stub in libOSXWindowManagement libblz_commerce_sdk_plugin libcommerce_http_client libNativeApiMac; do
  install -Dm755 "$target/$stub.so" "$share/stubs/$stub.so"
done
install -Dm644 assets/client.config.in "$share/client.config.in"

install -Dm644 "data/$app_id.desktop" "$root$prefix/share/applications/$app_id.desktop"
install -Dm644 "data/$app_id.metainfo.xml" "$root$prefix/share/metainfo/$app_id.metainfo.xml"
install -Dm644 "$icon" "$root$prefix/share/icons/hicolor/scalable/apps/$app_id.svg"
for size in 128 256; do
  dir="$root$prefix/share/icons/hicolor/${size}x${size}/apps"
  install -d "$dir"
  rsvg-convert -w "$size" -h "$size" "$icon" -o "$dir/$app_id.png"
done
