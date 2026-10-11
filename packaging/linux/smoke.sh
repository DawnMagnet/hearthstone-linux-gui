#!/usr/bin/env bash
# Installs and starts the release artifacts on the current distribution.
# Run as root inside a distro container (see .github/workflows/packaging.yml):
#
#   packaging/linux/smoke.sh <dist-dir>
#
# 1. Installs the matching native package, checks every library resolves and
#    the launcher starts (--no-gui and a few seconds of GUI under Xvfb).
# 2. Extracts the AppImage (on top of a plain desktop library set, without
#    GTK 4) and does the same, checking that bundled libraries come from the
#    AppImage and core libraries (glibc, GL, X11) from the host.
set -euo pipefail

dist="$(readlink -f "${1:?usage: smoke.sh <dist-dir>}")"
. /etc/os-release
echo "== ${PRETTY_NAME:-$ID}"

install_native() {
  case "$ID ${ID_LIKE:-}" in
    *debian* | *ubuntu*)
      export DEBIAN_FRONTEND=noninteractive
      apt-get update -qq
      apt-get install -y -qq "$dist"/*.deb xvfb libgtk-3-0 >/dev/null
      ;;
    *fedora* | *rhel* | *centos*)
      if [ "${VERSION_ID%%.*}" = 9 ] && [ "$ID" != fedora ]; then
        dnf -y -q install dnf-plugins-core >/dev/null
        dnf config-manager --set-enabled crb
      fi
      dnf -y -q --setopt=install_weak_deps=False install "$dist"/*.rpm xorg-x11-server-Xvfb gtk3 >/dev/null
      ;;
    *suse*)
      zypper --non-interactive --quiet install --allow-unsigned-rpm "$dist"/*.rpm xorg-x11-server-Xvfb gtk3-tools >/dev/null
      ;;
    *arch*)
      pacman -Syu --noconfirm --needed xorg-server-xvfb gtk3 >/dev/null
      pacman -U --noconfirm "$dist"/*.pkg.tar.zst >/dev/null
      ;;
    *)
      echo "unsupported distribution: $ID" >&2
      exit 1
      ;;
  esac
}

check_ldd() {
  local missing
  missing="$(find "$@" -type f \( -name '*.so*' -o -perm -u+x \) -print0 \
    | xargs -0 -r sh -c 'for f; do head -c4 "$f" | grep -q ELF && ldd "$f" 2>/dev/null | sed "s|^|$f: |"; done' sh \
    | grep 'not found' || true)"
  if [ -n "$missing" ]; then
    echo "$missing" >&2
    return 1
  fi
}

# Starts the GUI on a virtual display; it must still be running after 8 s.
gui_smoke() {
  local display=:99 status=0
  Xvfb "$display" -screen 0 1280x800x24 >/dev/null 2>&1 &
  local xvfb=$!
  sleep 2
  DISPLAY="$display" GDK_BACKEND=x11 timeout 8 "$@" || status=$?
  kill "$xvfb" 2>/dev/null || true
  if [ "$status" -ne 124 ]; then
    echo "error: '$*' exited with status $status before the timeout" >&2
    return 1
  fi
  echo "GUI stayed up: $*"
}

install_native
ldd --version | head -n1

echo "-- native package"
check_ldd /usr/bin/hearthstone-linux-gui /usr/share/hearthstone-linux-gui/stubs
hearthstone-linux-gui --no-gui
gui_smoke hearthstone-linux-gui

echo "-- AppImage"
work="$(mktemp -d)"
cp "$dist"/*.AppImage "$work/app.AppImage"
chmod +x "$work/app.AppImage"
(cd "$work" && ./app.AppImage --appimage-extract >/dev/null)
appdir="$work/squashfs-root"
check_ldd "$appdir/usr/bin" "$appdir/usr/lib"
# Bundled GTK must come from the AppImage, glibc from the host.
ldd "$appdir/usr/bin/hearthstone-linux-gui" | grep -E 'libgtk-4|libc\.so'
ldd "$appdir/usr/bin/hearthstone-linux-gui" | grep libgtk-4 | grep -q "$appdir"
ldd "$appdir/usr/bin/hearthstone-linux-gui" | grep 'libc\.so' | grep -vq "$appdir"
"$appdir/AppRun" --no-gui
gui_smoke "$appdir/AppRun"
echo "== OK: ${PRETTY_NAME:-$ID}"
