#!/usr/bin/env bash
set -euo pipefail

echo "Building release binaries"
cargo build --workspace --release
ldd target/release/hearthstone-linux-gui | tee /tmp/hearthstone-linux-gui.ldd
if grep -Eq 'libgtk-[34]\.so|libadwaita-1\.so' /tmp/hearthstone-linux-gui.ldd; then
  echo "unexpected GTK/libadwaita dependency in FLTK application" >&2
  exit 1
fi
echo "Running release tests"
cargo test --workspace --release

target_arch="${TARGETARCH:-amd64}"
case "$target_arch" in
  amd64)
    nfpm_arch=amd64
    deb_arch=amd64
    rpm_arch=x86_64
    pacman_arch=x86_64
    appimage_arch=x86_64
    ;;
  arm64)
    nfpm_arch=arm64
    deb_arch=arm64
    rpm_arch=aarch64
    pacman_arch=aarch64
    appimage_arch=
    ;;
  *)
    echo "unsupported TARGETARCH=$target_arch" >&2
    exit 1
    ;;
esac

PKG_ROOT="${PKG_ROOT:-./pkgroot}"
DIST_DIR="${DIST_DIR:-./dist}"

echo "Installing package root"
mkdir -p "$PKG_ROOT"
install -Dm755 target/release/hearthstone-linux-gui "$PKG_ROOT/usr/bin/hearthstone-linux-gui"
install -Dm755 target/release/libCoreFoundation.so "$PKG_ROOT/usr/share/hearthstone-linux-gui/stubs/CoreFoundation.so"
install -Dm755 target/release/libOSXWindowManagement.so "$PKG_ROOT/usr/share/hearthstone-linux-gui/stubs/libOSXWindowManagement.so"
install -Dm755 target/release/libblz_commerce_sdk_plugin.so "$PKG_ROOT/usr/share/hearthstone-linux-gui/stubs/libblz_commerce_sdk_plugin.so"
install -Dm644 assets/client.config.in "$PKG_ROOT/usr/share/hearthstone-linux-gui/client.config.in"
install -Dm644 data/io.github.hearthstone_linux_gui.desktop "$PKG_ROOT/usr/share/applications/io.github.hearthstone_linux_gui.desktop"
install -Dm644 data/io.github.hearthstone_linux_gui.metainfo.xml "$PKG_ROOT/usr/share/metainfo/io.github.hearthstone_linux_gui.metainfo.xml"
install -Dm644 packaging/appimage/io.github.hearthstone_linux_gui.svg "$PKG_ROOT/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg"
install -d "$PKG_ROOT/usr/share/icons/hicolor/128x128/apps" "$PKG_ROOT/usr/share/icons/hicolor/256x256/apps"
rsvg-convert -w 128 -h 128 packaging/appimage/io.github.hearthstone_linux_gui.svg -o "$PKG_ROOT/usr/share/icons/hicolor/128x128/apps/io.github.hearthstone_linux_gui.png"
rsvg-convert -w 256 -h 256 packaging/appimage/io.github.hearthstone_linux_gui.svg -o "$PKG_ROOT/usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png"

version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
mkdir -p "$DIST_DIR" /tmp/nfpm

cat > /tmp/nfpm/nfpm.yaml <<EOF
name: hearthstone-linux-gui
arch: ${nfpm_arch}
platform: linux
version: ${version}
section: games
priority: optional
maintainer: hearthstone-linux-gui contributors
homepage: https://github.com/DawnMagnet/hearthstone-linux
license: MIT
description: Native FLTK Linux GUI manager for installing, logging into, and launching Hearthstone.
contents:
  - src: ${PKG_ROOT}/usr/bin/hearthstone-linux-gui
    dst: /usr/bin/hearthstone-linux-gui
  - src: ${PKG_ROOT}/usr/share/hearthstone-linux-gui
    dst: /usr/share/hearthstone-linux-gui
  - src: ${PKG_ROOT}/usr/share/applications/io.github.hearthstone_linux_gui.desktop
    dst: /usr/share/applications/io.github.hearthstone_linux_gui.desktop
  - src: ${PKG_ROOT}/usr/share/metainfo/io.github.hearthstone_linux_gui.metainfo.xml
    dst: /usr/share/metainfo/io.github.hearthstone_linux_gui.metainfo.xml
  - src: ${PKG_ROOT}/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg
    dst: /usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg
  - src: ${PKG_ROOT}/usr/share/icons/hicolor/128x128/apps/io.github.hearthstone_linux_gui.png
    dst: /usr/share/icons/hicolor/128x128/apps/io.github.hearthstone_linux_gui.png
  - src: ${PKG_ROOT}/usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png
    dst: /usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png
overrides:
  deb:
    depends:
      - libc6 (>= 2.36)
      - libstdc++6
      - libx11-6
      - libxext6
      - libxinerama1
      - libxcursor1
      - libxrender1
      - libxfixes3
      - libcairo2
      - libpango-1.0-0
      - libpangocairo-1.0-0
      - libglib2.0-0 (>= 2.74)
      - libfontconfig1
      - libfreetype6
      - xdg-utils
  rpm:
    depends:
      - glibc
      - libgcc
      - libstdc++
      - libX11
      - libXext
      - libXinerama
      - libXcursor
      - libXrender
      - libXfixes
      - cairo
      - pango
      - glib2
      - fontconfig
      - freetype
      - xdg-utils
  archlinux:
    depends:
      - glibc
      - gcc-libs
      - libx11
      - libxext
      - libxinerama
      - libxcursor
      - libxrender
      - libxfixes
      - cairo
      - pango
      - glib2
      - fontconfig
      - freetype2
      - xdg-utils
EOF

deb_file="$DIST_DIR/hearthstone-linux-gui_${version}_${deb_arch}.deb"
rpm_file="$DIST_DIR/hearthstone-linux-gui-${version}-1.${rpm_arch}.rpm"
pacman_file="$DIST_DIR/hearthstone-linux-gui-${version}-1-${pacman_arch}.pkg.tar.zst"

nfpm package --config /tmp/nfpm/nfpm.yaml --packager deb --target "$deb_file"
nfpm package --config /tmp/nfpm/nfpm.yaml --packager rpm --target "$rpm_file"
nfpm package --config /tmp/nfpm/nfpm.yaml --packager archlinux --target "$pacman_file"

echo "Validating native packages"
test -s "$deb_file"
dpkg-deb --info "$deb_file" >/dev/null
test -s "$rpm_file"
rpm -qp --info "$rpm_file" >/dev/null
test -s "$pacman_file"
tar -tf "$pacman_file" .PKGINFO >/dev/null

if [ "$target_arch" = "amd64" ]; then
  echo "Building AppImage with linuxdeploy"
  appdir=/tmp/hearthstone-linux-gui.AppDir
  multiarch="$(dpkg-architecture -qDEB_HOST_MULTIARCH)"
  appimage_file="hearthstone-linux-gui-${version}-${appimage_arch}.AppImage"

  rm -rf "$appdir"
  mkdir -p "$appdir"
  cp -a "$PKG_ROOT/." "$appdir"/
  install -Dm755 packaging/appimage/AppRun "$appdir/AppRun"
  ln -s usr/share/applications/io.github.hearthstone_linux_gui.desktop "$appdir/io.github.hearthstone_linux_gui.desktop"
  ln -s usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg "$appdir/io.github.hearthstone_linux_gui.svg"
  ln -s usr/share/icons/hicolor/256x256/apps/io.github.hearthstone_linux_gui.png "$appdir/io.github.hearthstone_linux_gui.png"
  ln -s io.github.hearthstone_linux_gui.png "$appdir/.DirIcon"
  if [ -d /usr/share/X11/xkb ]; then
    mkdir -p "$appdir/usr/share/X11"
    cp -a /usr/share/X11/xkb "$appdir/usr/share/X11/"
  fi

  rm -rf /tmp/linuxdeploy-output
  mkdir -p /tmp/linuxdeploy-output
  (
    cd /tmp/linuxdeploy-output
    export APPIMAGE_EXTRACT_AND_RUN=1
    export LD_LIBRARY_PATH="$appdir/usr/lib:$appdir/usr/lib/$multiarch:/usr/lib/$multiarch:/lib/$multiarch${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    export LINUXDEPLOY_OUTPUT_VERSION="$version"
    /opt/linuxdeploy/usr/bin/linuxdeploy \
      --appdir "$appdir" \
      --executable "$appdir/usr/bin/hearthstone-linux-gui" \
      --desktop-file "$appdir/usr/share/applications/io.github.hearthstone_linux_gui.desktop" \
      --icon-file "$appdir/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux_gui.svg"
    /opt/linuxdeploy/usr/bin/linuxdeploy-plugin-appimage \
      --appdir "$appdir"
  )

  produced="$(find /tmp/linuxdeploy-output -maxdepth 1 -type f -name '*.AppImage' -print -quit)"
  if [ -z "$produced" ]; then
    echo "linuxdeploy did not produce an AppImage" >&2
    exit 1
  fi
  install -Dm755 "$produced" "$DIST_DIR/$appimage_file"

  extract_appimage() {
    local image="$1" dest="$2" offset
    rm -rf "$dest"
    mkdir -p "$dest"
    if (cd "$dest" && "$image" --appimage-extract >/dev/null 2>&1); then
      return 0
    fi
    while IFS=: read -r offset _; do
      [ -n "$offset" ] || continue
      if unsquashfs -s -o "$offset" "$image" >/dev/null 2>&1; then
        unsquashfs -q -o "$offset" -d "$dest/squashfs-root" "$image"
        return 0
      fi
    done < <(grep -abo hsqs "$image" || true)
    echo "could not find a valid SquashFS payload in $image" >&2
    exit 1
  }

  if ! APPIMAGE_EXTRACT_AND_RUN=1 "$DIST_DIR/$appimage_file" --no-gui; then
    echo "Direct AppImage execution failed in the container, retrying extracted AppRun smoke test"
    extract_appimage "$DIST_DIR/$appimage_file" /tmp/appimage-run
    APPDIR=/tmp/appimage-run/squashfs-root /tmp/appimage-run/squashfs-root/AppRun --no-gui
  fi

  echo "Scanning extracted AppImage ELF dependencies"
  extract_appimage "$DIST_DIR/$appimage_file" /tmp/appimage-smoke
  while IFS= read -r -d "" elf; do
    readelf -h "$elf" >/dev/null 2>&1 || continue
    LD_LIBRARY_PATH="/tmp/appimage-smoke/squashfs-root/usr/lib:/tmp/appimage-smoke/squashfs-root/usr/lib/$multiarch" \
      ldd "$elf" > /tmp/ldd.out
    if grep -q "not found" /tmp/ldd.out; then
      echo "missing dependencies for $elf" >&2
      cat /tmp/ldd.out >&2
      exit 1
    fi
  done < <(find /tmp/appimage-smoke/squashfs-root/usr/bin /tmp/appimage-smoke/squashfs-root/usr/lib -type f -print0)
fi

cat > "$DIST_DIR/README.txt" <<'EOF'
hearthstone-linux-gui distribution artifacts

*.AppImage is built in the Debian Dockerfile with linuxdeploy after the native package formats are produced.
*.deb installs the native FLTK executable and desktop integration.
*.rpm installs the native FLTK executable and desktop integration.
*.pkg.tar.zst installs the native FLTK executable and desktop integration on pacman-based systems.
EOF

(
  cd "$DIST_DIR"
  find . -maxdepth 1 -type f ! -name SHA256SUMS.txt -printf '%P\0' \
    | sort -z \
    | xargs -0 sha256sum > SHA256SUMS.txt
)
