#!/usr/bin/env bash
# Installs the release build into an FHS tree (default /pkgroot/usr/...).
# Shared by the deb/rpm/pacman packages and the AppImage.
set -euo pipefail

root="${1:?usage: stage.sh <pkgroot>}"
target="${CARGO_TARGET_DIR:-target}/release"
app_id=io.github.hearthstone_linux_gui
share="$root/usr/share/hearthstone-linux-gui"
icon=packaging/appimage/$app_id.svg

install -Dm755 "$target/hearthstone-linux-gui" "$root/usr/bin/hearthstone-linux-gui"

# Stub libraries loaded into the Unity player by the host's dynamic loader.
install -Dm755 "$target/libCoreFoundation.so" "$share/stubs/CoreFoundation.so"
for stub in libOSXWindowManagement libblz_commerce_sdk_plugin libcommerce_http_client libNativeApiMac; do
  install -Dm755 "$target/$stub.so" "$share/stubs/$stub.so"
done
install -Dm644 assets/client.config.in "$share/client.config.in"

install -Dm644 "data/$app_id.desktop" "$root/usr/share/applications/$app_id.desktop"
install -Dm644 "data/$app_id.metainfo.xml" "$root/usr/share/metainfo/$app_id.metainfo.xml"
install -Dm644 "$icon" "$root/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
for size in 128 256; do
  dir="$root/usr/share/icons/hicolor/${size}x${size}/apps"
  install -d "$dir"
  rsvg-convert -w "$size" -h "$size" "$icon" -o "$dir/$app_id.png"
done
