#!/usr/bin/env bash
# Builds deb, rpm and pacman packages from a staged tree. The packages use the
# host's GTK/libadwaita/GLib; the binary only needs glibc >= 2.34.
#
# Library floors come from the gtk-rs feature levels selected in Cargo.toml
# (relm4 without its `gnome_42` default): GTK 4.0, libadwaita 1.0, GLib 2.66,
# Pango 1.46. In practice the oldest distributions that ship GTK 4 and
# libadwaita are Ubuntu 22.04 and Enterprise Linux 9.
#
# Native packages are x86_64 only: Hearthstone's Unity player has no other
# Linux architecture. RPM dependencies use soname provides so the same
# package resolves on Fedora, EL9 and openSUSE.
set -euo pipefail

case "$(uname -m)" in
  x86_64 | amd64) ;;
  *)
    echo "error: native packages are x86_64 only (got $(uname -m))" >&2
    exit 1
    ;;
esac

root="${1:?usage: packages.sh <pkgroot> <outdir>}"
out="${2:?usage: packages.sh <pkgroot> <outdir>}"
version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
app_id=io.github.hearthstone_linux_gui
config="$(mktemp -d)/nfpm.yaml"
mkdir -p "$out"

cat > "$config" <<YAML
name: hearthstone-linux-gui
arch: amd64
platform: linux
version: ${version}
section: games
priority: optional
maintainer: hearthstone-linux-gui contributors
homepage: https://github.com/DawnMagnet/hearthstone-linux-gui
license: MIT
description: Native GTK4 Linux GUI manager for installing, logging into, and launching Hearthstone.
contents:
  - src: ${root}/usr/bin/hearthstone-linux-gui
    dst: /usr/bin/hearthstone-linux-gui
  - src: ${root}/usr/share/hearthstone-linux-gui
    dst: /usr/share/hearthstone-linux-gui
  - src: ${root}/usr/share/applications/${app_id}.desktop
    dst: /usr/share/applications/${app_id}.desktop
  - src: ${root}/usr/share/metainfo/${app_id}.metainfo.xml
    dst: /usr/share/metainfo/${app_id}.metainfo.xml
  - src: ${root}/usr/share/icons/hicolor/scalable/apps/${app_id}.svg
    dst: /usr/share/icons/hicolor/scalable/apps/${app_id}.svg
  - src: ${root}/usr/share/icons/hicolor/128x128/apps/${app_id}.png
    dst: /usr/share/icons/hicolor/128x128/apps/${app_id}.png
  - src: ${root}/usr/share/icons/hicolor/256x256/apps/${app_id}.png
    dst: /usr/share/icons/hicolor/256x256/apps/${app_id}.png
overrides:
  deb:
    depends:
      - libc6 (>= 2.34)
      - libgtk-4-1 (>= 4.0)
      - libadwaita-1-0 (>= 1.0)
      - libglib2.0-0 (>= 2.66) | libglib2.0-0t64 (>= 2.66)
      - libpango-1.0-0 (>= 1.46) | libpango-1.0-0t64 (>= 1.46)
      - xdg-utils
  rpm:
    # soname dependencies resolve on Fedora, EL9 and openSUSE alike.
    depends:
      - libc.so.6(GLIBC_2.34)(64bit)
      - libgtk-4.so.1()(64bit)
      - libadwaita-1.so.0()(64bit)
      - libglib-2.0.so.0()(64bit)
      - libgio-2.0.so.0()(64bit)
      - libpango-1.0.so.0()(64bit)
      - xdg-utils
  archlinux:
    depends:
      - glibc
      - gtk4
      - libadwaita
      - glib2
      - pango
      - xdg-utils
YAML

nfpm package --config "$config" --packager deb --target "$out/hearthstone-linux-gui_${version}_amd64.deb"
nfpm package --config "$config" --packager rpm --target "$out/hearthstone-linux-gui-${version}-1.x86_64.rpm"
nfpm package --config "$config" --packager archlinux --target "$out/hearthstone-linux-gui-${version}-1-x86_64.pkg.tar.zst"
