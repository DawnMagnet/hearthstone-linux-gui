#!/usr/bin/env bash
# Headless Wayland smoke for the glibc 2.28 desktops (Debian 11, Ubuntu 20.04).
# Called from smoke.sh after the X11 AppImage checks. Runs as root in the
# distro container.
#
# Mutter on these releases has no --headless / --virtual-monitor (that landed
# in mutter 41/42). The script reads `mutter --help` and, when those flags are
# absent, launches Weston headless instead and records the substitution.
# kwin_wayland on these releases does have a virtual backend; the script uses
# the flags that binary actually accepts (`--virtual` plus width/height).
#
# The GUI is started with --appimage-extract-and-run and GDK_BACKEND=wayland.
# It must stay up for 8 seconds, keep a socket to the compositor, and have
# wayland-compat on its own LD_LIBRARY_PATH (host Wayland 1.18). --print-child-env
# must not contain wayland-compat. Screenshots are best-effort (grim, then the
# compositor's own tool) and land in WAYLAND_SCREENSHOT_DIR.
set -euo pipefail

appimage="$(readlink -f "${1:?usage: wayland-smoke.sh <AppImage>}")"
# smoke.sh keeps the AppImage in a root-only mktemp directory. The Wayland
# session runs as an unprivileged user and must be able to traverse that path.
chmod a+rx "$(dirname "$appimage")" "$appimage" 2>/dev/null || true

# shellcheck disable=SC1091
. /etc/os-release

shot_dir="${WAYLAND_SCREENSHOT_DIR:-$PWD/wayland-screenshots}"
mkdir -p "$shot_dir"
summary="$shot_dir/summary.txt"
: > "$summary"
note() {
  printf '%s\n' "$*" | tee -a "$summary"
}

note "== Wayland smoke on ${PRETTY_NAME:-$ID}"

export DEBIAN_FRONTEND=noninteractive

# Debian 11's image ships a security-updated perl-base, while bullseye main
# (the only suite left after bullseye-security is removed) has an older perl
# that depends on that exact older perl-base. kwin-wayland pulls perl.
if [ "${ID:-}" = debian ] && [ "${VERSION_ID:-}" = 11 ]; then
  # Depends: perl-base (= 5.32.1-4+deb11u3), perl-modules-5.32 (>= ...), ...
  base_ver="$(apt-cache show perl | sed -n 's/^Depends: perl-base (= \([^)]*\)).*/\1/p')"
  base_ver="${base_ver%%$'\n'*}"
  if [ -n "$base_ver" ]; then
    note "downgrading perl-base to ${base_ver} so kwin-wayland matches bullseye main"
    apt-get install -y -qq --no-install-recommends --allow-downgrades \
      "perl-base=${base_ver}" perl >/dev/null
  fi
fi

apt-get install -y -qq --no-install-recommends \
  mutter kwin-wayland kwin-wayland-backend-virtual weston grim \
  dbus libcap2-bin libgl1-mesa-dri passwd >/dev/null

pkg_ver() {
  dpkg-query -W -f '${Version}' "$1" 2>/dev/null || printf 'not-installed'
}

note "package mutter $(pkg_ver mutter)"
note "package kwin-wayland $(pkg_ver kwin-wayland)"
note "package kwin-wayland-backend-virtual $(pkg_ver kwin-wayland-backend-virtual)"
note "package weston $(pkg_ver weston)"
note "package grim $(pkg_ver grim)"

if ! id wlsmoke >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash wlsmoke
fi
chown wlsmoke "$shot_dir"

# File capabilities make execve return EPERM inside a container.
kwin_caps="$(getcap /usr/bin/kwin_wayland 2>/dev/null || true)"
if [ -n "$kwin_caps" ]; then
  note "clearing file capabilities on ${kwin_caps}"
  setcap -r /usr/bin/kwin_wayland
fi

mutter_help="$(mutter --help 2>&1 || true)"
mutter_ver="$(mutter --version 2>&1 || true)"
mutter_ver="${mutter_ver%%$'\n'*}"
note "mutter version: ${mutter_ver}"

kwin_help="$(runuser -u wlsmoke -- timeout 15 kwin_wayland --help 2>&1 || true)"
kwin_ver="$(dpkg-query -W -f '${Version}' kwin-wayland)"
note "kwin_wayland package version: ${kwin_ver}"

weston_help="$(weston --help 2>&1 || true)"
weston_ver="$(weston --version 2>&1 || true)"
weston_ver="${weston_ver%%$'\n'*}"
note "weston version: ${weston_ver}"

# Weston command used when mutter or kwin cannot provide a virtual output.
# Flags are taken from this weston's --help (headless-backend.so on 8.x/9.x).
weston_cmd=(weston --backend=headless-backend.so --socket=wayland-0 --no-config)
case "$weston_help" in
  *--width*) weston_cmd+=(--width=1280 --height=800) ;;
esac
case "$weston_help" in
  *--use-pixman*) weston_cmd+=(--use-pixman) ;;
esac
# External weston-screenshooter is rejected unless the debug protocol is on:
# "screenshooter failed: permission denied. Debug protocol must be enabled".
case "$weston_help" in
  *--debug*) weston_cmd+=(--debug) ;;
esac

gnome_note=""
case "$mutter_help" in
  *--headless*--virtual-monitor* | *--virtual-monitor*--headless*)
    gnome_cmd=(mutter --headless --wayland --virtual-monitor 1280x800)
    gnome_shot=mutter
    gnome_note="GNOME: ${mutter_ver} supports --headless --virtual-monitor; command: ${gnome_cmd[*]}"
    ;;
  *)
    gnome_cmd=("${weston_cmd[@]}")
    gnome_shot=weston
    gnome_note="GNOME: ${mutter_ver} has no --headless or --virtual-monitor (present flags include --wayland --nested --display-server --no-x11). Substitute: ${weston_ver} ${weston_cmd[*]}"
    ;;
esac
note "$gnome_note"

kde_note=""
case "$kwin_help" in
  *--virtual*)
    kde_cmd=(kwin_wayland --virtual)
    case "$kwin_help" in
      *--socket*) kde_cmd+=(--socket wayland-0) ;;
    esac
    case "$kwin_help" in
      *--width*) kde_cmd+=(--width 1280) ;;
    esac
    case "$kwin_help" in
      *--height*) kde_cmd+=(--height 800) ;;
    esac
    kde_shot=kwin
    kde_note="KDE: kwin-wayland ${kwin_ver} supports --virtual; command: ${kde_cmd[*]}"
    ;;
  *)
    kde_cmd=("${weston_cmd[@]}")
    kde_shot=weston
    kde_note="KDE: kwin-wayland ${kwin_ver} --help has no --virtual. Substitute: ${weston_ver} ${weston_cmd[*]}"
    ;;
esac
note "$kde_note"
if [ "$kde_shot" = weston ]; then
  note "-- kwin_wayland --help (first lines) --"
  printf '%s\n' "$kwin_help" | awk 'NR<=40{print}' | tee -a "$summary"
fi

note "-- child environment (--appimage-extract-and-run --print-child-env)"
child="$(runuser -u wlsmoke -- env -u LD_LIBRARY_PATH -u WAYLAND_DISPLAY -u DISPLAY \
  "$appimage" --appimage-extract-and-run --print-child-env)"
printf '%s\n' "$child" | tee -a "$summary"
case "$child" in
  *$'# host_env_active=1'*) ;;
  *)
    note "error: --print-child-env missing host_env_active=1"
    exit 1
    ;;
esac
case "$child" in
  *wayland-compat*)
    note "error: game child environment contains wayland-compat"
    exit 1
    ;;
esac
note "child environment has no wayland-compat"

session="$(mktemp)"
cat > "$session" << 'EOS'
#!/bin/bash
set -u
runtime=$1
appimage=$2
shot=$3
label=$4
shot_tool=$5
shift 5
export XDG_RUNTIME_DIR=$runtime
export WAYLAND_DISPLAY=wayland-0
unset DISPLAY
export LIBGL_ALWAYS_SOFTWARE=1
export GALLIUM_DRIVER=llvmpipe
export MESA_LOADER_DRIVER_OVERRIDE=llvmpipe
"$@" > "$runtime/compositor.log" 2>&1 &
comp=$!
ready=0
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
  if [ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]; then
    ready=1
    break
  fi
  if ! kill -0 "$comp" 2>/dev/null; then
    break
  fi
  sleep 0.2
done
if [ "$ready" != 1 ]; then
  echo "error: ${label} compositor did not create ${WAYLAND_DISPLAY}" >&2
  cat "$runtime/compositor.log" >&2 || true
  exit 1
fi
export GDK_BACKEND=wayland
export GSK_RENDERER=cairo
timeout 8 "$appimage" --appimage-extract-and-run > "$runtime/app.log" 2>&1 &
app=$!
gui=""
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24; do
  for d in /proc/[0-9]*; do
    comm=""
    comm=$(cat "$d/comm" 2>/dev/null || true)
    case "$comm" in
      hearthstone-lin*)
        gui=${d#/proc/}
        break
        ;;
    esac
  done
  [ -n "$gui" ] && break
  sleep 0.25
done
if [ -z "$gui" ]; then
  echo "error: ${label} GUI process did not appear" >&2
  cat "$runtime/app.log" >&2 || true
  kill "$comp" 2>/dev/null || true
  exit 1
fi
ldp=""
while IFS= read -r line; do
  case "$line" in
    LD_LIBRARY_PATH=*) ldp=${line#LD_LIBRARY_PATH=} ;;
  esac
done < <(tr '\0' '\n' < "/proc/$gui/environ")
printf 'GUI LD_LIBRARY_PATH=%s\n' "$ldp"
case "$ldp" in
  *wayland-compat*) ;;
  *)
    echo "error: ${label} GUI LD_LIBRARY_PATH missing wayland-compat" >&2
    kill "$app" "$comp" 2>/dev/null || true
    exit 1
    ;;
esac
# A connected Wayland socket shows up as socket:[inode], not as a path.
# Match that inode to the compositor socket in /proc/net/unix. libwayland-cursor
# also maps a memfd named wayland-cursor once the Wayland backend is up.
found=0
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  for fd in /proc/"$gui"/fd/*; do
    target=""
    target=$(readlink "$fd" 2>/dev/null || true)
    case "$target" in
      *"$WAYLAND_DISPLAY"* | *wayland-cursor*) found=1 ;;
      socket:\[*\])
        ino=${target#socket:[}
        ino=${ino%]}
        while read -r _n _ref _proto _flags _type _st inode path; do
          if [ "$inode" = "$ino" ]; then
            case "$path" in
              *"$WAYLAND_DISPLAY"*) found=1 ;;
            esac
          fi
        done < /proc/net/unix
        ;;
    esac
  done
  [ "$found" = 1 ] && break
  if ! kill -0 "$gui" 2>/dev/null; then
    break
  fi
  sleep 0.25
done
if [ "$found" != 1 ]; then
  echo "error: ${label} GUI is not connected to ${WAYLAND_DISPLAY}" >&2
  echo "-- open fds --" >&2
  for fd in /proc/"$gui"/fd/*; do
    readlink "$fd" 2>/dev/null || true
  done >&2
  kill "$app" "$comp" 2>/dev/null || true
  exit 1
fi
echo "GUI wayland socket fd: yes"
if grep -E -i 'x11 backend|fell back to x11|gdk_x11_display|using the x11 backend' "$runtime/app.log"; then
  echo "error: ${label} log shows an X11 fallback" >&2
  kill "$app" "$comp" 2>/dev/null || true
  exit 1
fi
sleep 1
rm -f "$shot"
if [ "$shot_tool" = weston ] && command -v weston-screenshooter >/dev/null 2>&1; then
  # Weston 8/9 ignores the filename and writes wayland-screenshot-<stamp>.png
  # in the current directory. --debug on the compositor is required.
  (
    cd "$runtime"
    weston-screenshooter
  ) > "$runtime/screenshooter.log" 2>&1 || true
  for candidate in "$runtime"/wayland-screenshot-*.png "$runtime"/wayland-screenshot.png "$runtime"/screenshot.png; do
    [ -s "$candidate" ] || continue
    cp "$candidate" "$shot"
    break
  done
fi
if [ ! -s "$shot" ] && [ "$shot_tool" = kwin ] && [ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
  reply=""
  reply=$(dbus-send --session --print-reply --dest=org.kde.KWin /Screenshot \
    org.kde.kwin.Screenshot.screenshotFullscreen 2>"$runtime/dbus.err" || true)
  path=""
  while IFS= read -r line; do
    case "$line" in
      *'string "'*)
        path=${line#*string \"}
        path=${path%\"}
        break
        ;;
    esac
  done <<< "$reply"
  if [ -n "$path" ] && [ -s "$path" ]; then
    cp "$path" "$shot"
  fi
  printf '%s\n' "$reply" > "$runtime/dbus.txt"
  if [ ! -s "$shot" ]; then
    reply=$(dbus-send --session --print-reply --dest=org.kde.KWin /Screenshot \
      org.kde.kwin.Screenshot.screenshotFullscreen boolean:false 2>>"$runtime/dbus.err" || true)
    path=""
    while IFS= read -r line; do
      case "$line" in
        *'string "'*)
          path=${line#*string \"}
          path=${path%\"}
          break
          ;;
      esac
    done <<< "$reply"
    if [ -n "$path" ] && [ -s "$path" ]; then
      cp "$path" "$shot"
    fi
    printf '%s\n' "$reply" >> "$runtime/dbus.txt"
  fi
fi
if [ ! -s "$shot" ] && command -v grim >/dev/null 2>&1; then
  grim "$shot" > "$runtime/grim.log" 2>&1 || true
fi
if [ -s "$shot" ]; then
  echo "screenshot: $shot"
else
  echo "screenshot: unavailable for ${label} (grim and ${shot_tool} tool produced no file)"
fi
set +e
wait "$app"
status=$?
set -e
kill "$comp" 2>/dev/null || true
wait "$comp" 2>/dev/null || true
if [ "$status" -ne 124 ]; then
  echo "error: ${label} GUI exited with status ${status} before the 8s timeout" >&2
  cat "$runtime/app.log" >&2 || true
  exit 1
fi
echo "GUI stayed up on wayland (${label})"
EOS
chmod 755 "$session"

run_session() {
  local label=$1 shot_tool=$2
  shift 2
  local runtime shot
  runtime="$(mktemp -d /tmp/wl-runtime.XXXXXX)"
  chmod 700 "$runtime"
  chown wlsmoke "$runtime"
  shot="$shot_dir/${label}.png"
  note "-- session ${label}: $*"
  set +e
  runuser -u wlsmoke -- env XDG_RUNTIME_DIR="$runtime" \
    dbus-run-session -- bash "$session" \
    "$runtime" "$appimage" "$shot" "$label" "$shot_tool" "$@"
  local status=$?
  set -e
  if [ -f "$runtime/app.log" ]; then
    cp "$runtime/app.log" "$shot_dir/${label}-app.log"
  fi
  if [ -f "$runtime/compositor.log" ]; then
    cp "$runtime/compositor.log" "$shot_dir/${label}-compositor.log"
  fi
  if [ "$status" -ne 0 ]; then
    note "error: ${label} session exited ${status}"
    if [ -f "$runtime/compositor.log" ]; then
      note "-- ${label} compositor log --"
      tee -a "$summary" < "$runtime/compositor.log" >&2
    fi
    if [ -f "$runtime/app.log" ]; then
      note "-- ${label} app log --"
      tee -a "$summary" < "$runtime/app.log" >&2
    fi
    exit 1
  fi
  if [ -s "$shot" ]; then
    note "screenshot saved: ${shot}"
  else
    note "screenshot unavailable for ${label}"
  fi
}

run_session "gnome-${gnome_shot}" "$gnome_shot" "${gnome_cmd[@]}"
run_session "kde-${kde_shot}" "$kde_shot" "${kde_cmd[@]}"

note "== Wayland smoke OK"
rm -f "$session"
