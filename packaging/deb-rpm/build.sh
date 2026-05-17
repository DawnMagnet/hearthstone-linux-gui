#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APPIMAGE="${1:-}"
VERSION="${VERSION:-0.1.0}"
DIST="${DIST:-$ROOT/dist/packages}"
WORK="$(mktemp -d)"

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT

if [ -z "$APPIMAGE" ] || [ ! -f "$APPIMAGE" ]; then
  echo "usage: $0 path/to/Hearthstone_Linux-x86_64.AppImage" >&2
  exit 1
fi

if ! command -v nfpm >/dev/null 2>&1; then
  echo "missing required command: nfpm" >&2
  exit 1
fi

mkdir -p "$DIST" "$WORK/root/opt/hearthstone-linux" "$WORK/root/usr/bin" \
  "$WORK/root/usr/share/applications" "$WORK/root/usr/share/icons/hicolor/scalable/apps"

install -Dm755 "$APPIMAGE" "$WORK/root/opt/hearthstone-linux/Hearthstone_Linux-x86_64.AppImage"
install -Dm644 "$ROOT/data/io.github.hearthstone_linux.desktop" "$WORK/root/usr/share/applications/io.github.hearthstone_linux.desktop"
install -Dm644 "$ROOT/packaging/appimage/io.github.hearthstone_linux.svg" "$WORK/root/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux.svg"

cat > "$WORK/root/usr/bin/hearthstone-linux" <<'WRAPPER'
#!/usr/bin/env sh
export APPIMAGE_EXTRACT_AND_RUN="${APPIMAGE_EXTRACT_AND_RUN:-1}"
exec /opt/hearthstone-linux/Hearthstone_Linux-x86_64.AppImage "$@"
WRAPPER
chmod 755 "$WORK/root/usr/bin/hearthstone-linux"

cat > "$WORK/nfpm.yaml" <<EOF
name: hearthstone-linux
arch: amd64
platform: linux
version: "$VERSION"
section: games
priority: optional
maintainer: Hearthstone Linux contributors
description: Native Linux manager for installing, logging into, and launching Hearthstone.
license: MIT
contents:
  - src: $WORK/root/opt/hearthstone-linux/Hearthstone_Linux-x86_64.AppImage
    dst: /opt/hearthstone-linux/Hearthstone_Linux-x86_64.AppImage
    file_info:
      mode: 0755
  - src: $WORK/root/usr/bin/hearthstone-linux
    dst: /usr/bin/hearthstone-linux
    file_info:
      mode: 0755
  - src: $WORK/root/usr/share/applications/io.github.hearthstone_linux.desktop
    dst: /usr/share/applications/io.github.hearthstone_linux.desktop
  - src: $WORK/root/usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux.svg
    dst: /usr/share/icons/hicolor/scalable/apps/io.github.hearthstone_linux.svg
EOF

nfpm package --config "$WORK/nfpm.yaml" --packager deb --target "$DIST"
nfpm package --config "$WORK/nfpm.yaml" --packager rpm --target "$DIST"
