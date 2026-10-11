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
# 3. Runs the hidden `--print-child-env` probe so host children do not inherit
#    AppImage/bundled LD_LIBRARY_PATH or GTK/GLib module paths (issue #22).
#
# Avoid `cmd | head` / `cmd | grep -q` under `set -o pipefail`: short-circuit
# readers close the pipe and the writer exits 141 (SIGPIPE) on many distros.
set -euo pipefail

dist="$(readlink -f "${1:?usage: smoke.sh <dist-dir>}")"
. /etc/os-release
echo "== ${PRETTY_NAME:-$ID}"

# First line of a multi-line string without a head|pipe SIGPIPE.
first_line() {
  local s="$1"
  printf '%s\n' "${s%%$'\n'*}"
}

# True when file starts with the ELF magic (read the file, not a pipe).
is_elf() {
  [ "$(head -c4 "$1" 2>/dev/null || true)" = $'\x7fELF' ]
}

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
      dnf -y -q --setopt=install_weak_deps=False install "$dist"/*.rpm xorg-x11-server-Xvfb gtk3 libglvnd-gles mesa-dri-drivers >/dev/null
      ;;
    *suse*)
      zypper --non-interactive --quiet install --allow-unsigned-rpm "$dist"/*.rpm xorg-x11-server-Xvfb gtk3-tools libglvnd Mesa-dri >/dev/null
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
  local missing="" f line
  while IFS= read -r -d '' f; do
    is_elf "$f" || continue
    while IFS= read -r line; do
      case "$line" in
        *'not found'*)
          missing+="$f: $line"$'\n'
          ;;
      esac
    done < <(ldd "$f" 2>/dev/null || true)
  done < <(find "$@" -type f \( -name '*.so*' -o -perm -u+x \) -print0 2>/dev/null)
  if [ -n "$missing" ]; then
    printf '%s' "$missing" >&2
    return 1
  fi
}

# Starts the GUI on a virtual display; it must still be running after 8 s.
# Force GTK's Cairo renderer: headless Xvfb often lacks DRI3/GLES and Fedora /
# Tumbleweed otherwise abort or segfault when GTK tries the GL path.
gui_smoke() {
  local display=:99 status=0
  # Minimal machine-id so GLib/dbus warnings do not escalate on some images.
  if [ ! -s /etc/machine-id ]; then
    printf '%032x\n' "$$" >/etc/machine-id 2>/dev/null || true
  fi
  Xvfb "$display" -screen 0 1280x800x24 +extension GLX >/dev/null 2>&1 &
  local xvfb=$!
  sleep 2
  DISPLAY="$display" GDK_BACKEND=x11 GSK_RENDERER=cairo LIBGL_ALWAYS_SOFTWARE=1     timeout 8 "$@" || status=$?
  kill "$xvfb" 2>/dev/null || true
  if [ "$status" -ne 124 ]; then
    echo "error: '$*' exited with status $status before the timeout" >&2
    return 1
  fi
  echo "GUI stayed up: $*"
}

# Verifies `--print-child-env`. Optional $2 = AppDir (AppImage). Optional $3 =
# expected LD_LIBRARY_PATH after restore (`<unset>` or an exact string).
# Variables AppRun forces into the AppDir must never appear in the child.
check_child_env() {
  local launcher="$1"
  local appdir="${2:-}"
  local expect_ld="${3:-}"
  local out value var

  echo "-- child environment ($launcher)"
  out="$("$launcher" --print-child-env)"
  printf '%s\n' "$out"

  case "$out" in
    *$'# host_env_active='*) ;;
    *)
      echo "error: --print-child-env missing host_env_active header" >&2
      return 1
      ;;
  esac
  case "$out" in
    *$'\nPATH='*|PATH=*) ;;
    *)
      echo "error: --print-child-env missing PATH" >&2
      return 1
      ;;
  esac

  child_var() {
    local prefix="$1=" line
    while IFS= read -r line; do
      case "$line" in
        "$prefix"*)
          printf '%s\n' "${line#"$prefix"}"
          return 0
          ;;
      esac
    done <<< "$out"
    printf '\n'
  }

  if [ -n "$expect_ld" ]; then
    value="$(child_var LD_LIBRARY_PATH)"
    if [ "$value" != "$expect_ld" ]; then
      echo "error: child LD_LIBRARY_PATH='$value', expected '$expect_ld'" >&2
      return 1
    fi
  fi

  if [ -n "$appdir" ]; then
    if ! grep -Fxq '# host_env_active=1' <<< "$out"; then
      echo "error: AppImage child probe expected host_env_active=1" >&2
      return 1
    fi

    # AppRun points these at the bundle; host children must get the host values.
    for var in GIO_MODULE_DIR GTK_PATH GTK_EXE_PREFIX GI_TYPELIB_PATH \
               GDK_PIXBUF_MODULE_FILE GSETTINGS_SCHEMA_DIR XDG_DATA_DIRS; do
      value="$(child_var "$var")"
      [ -z "$value" ] || [ "$value" = "<unset>" ] && continue
      case "$value" in
        "$appdir"|"$appdir"/*|*"$appdir"/*)
          echo "error: child $var still points into AppDir ($appdir): $value" >&2
          return 1
          ;;
      esac
    done

    # Without an explicit expect_ld, LD_LIBRARY_PATH must not look like an
    # AppImage mount. Poison tests pass expect_ld=/tmp/... instead.
    value="$(child_var LD_LIBRARY_PATH)"
    if [ -z "$expect_ld" ] || [ "$expect_ld" = "<unset>" ]; then
      if [ -n "$value" ] && [ "$value" != "<unset>" ]; then
        case "$value" in
          */squashfs-root/*|*/.mount_*/*)
            echo "error: child LD_LIBRARY_PATH points at an AppImage mount: $value" >&2
            return 1
            ;;
        esac
      fi
    fi
  fi

  echo "child environment OK"
}

install_native

ldd_ver="$(ldd --version 2>&1 || true)"
first_line "$ldd_ver"

echo "-- native package"
check_ldd /usr/bin/hearthstone-linux-gui /usr/share/hearthstone-linux-gui/stubs
hearthstone-linux-gui --no-gui
check_child_env hearthstone-linux-gui
gui_smoke hearthstone-linux-gui

echo "-- AppImage"
work="$(mktemp -d)"
cp "$dist"/*.AppImage "$work/app.AppImage"
chmod +x "$work/app.AppImage"
(cd "$work" && ./app.AppImage --appimage-extract >/dev/null)
appdir="$work/squashfs-root"
check_ldd "$appdir/usr/bin" "$appdir/usr/lib"

# Bundled GTK must come from the AppImage, glibc from the host.
ldd_bin="$(ldd "$appdir/usr/bin/hearthstone-linux-gui")"
printf '%s\n' "$ldd_bin" | grep -E 'libgtk-4|libc\.so' || true
gtk_line="$(printf '%s\n' "$ldd_bin" | grep libgtk-4 || true)"
libc_line="$(printf '%s\n' "$ldd_bin" | grep 'libc\.so' || true)"
case "$gtk_line" in
  *"$appdir"*) ;;
  *)
    echo "error: libgtk-4 is not resolved from AppDir ($appdir): $gtk_line" >&2
    exit 1
    ;;
esac
case "$libc_line" in
  *"$appdir"*)
    echo "error: libc.so must come from the host, not AppDir: $libc_line" >&2
    exit 1
    ;;
  '')
    echo "error: libc.so mapping missing from ldd output" >&2
    exit 1
    ;;
esac

"$appdir/AppRun" --no-gui
# Default host has no LD_LIBRARY_PATH: child must keep it unset and must not
# inherit AppRun's GTK/GLib module paths (issue #22).
check_child_env "$appdir/AppRun" "$appdir" "<unset>"
# Poison a non-AppDir LD_LIBRARY_PATH: AppRun records it, host_env restores it,
# and GTK/GLib vars still must not point into the AppDir.
poison="$(mktemp -d "$work/ld-poison.XXXXXX")"
LD_LIBRARY_PATH="$poison" check_child_env "$appdir/AppRun" "$appdir" "$poison"
gui_smoke "$appdir/AppRun"
echo "== OK: ${PRETTY_NAME:-$ID}"
