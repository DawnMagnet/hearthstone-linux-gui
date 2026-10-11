#!/usr/bin/env bash
# Fails when any ELF file under the given paths needs a glibc symbol version
# newer than <max>, or when a core host library that must never be bundled
# (glibc, ld-linux, libstdc++, libgcc_s, GL/EGL, libdrm, X11/xcb, Wayland)
# is present in the tree.
#
#   check-glibc.sh 2.34 /pkgroot AppDir
set -euo pipefail

max="${1:?usage: check-glibc.sh <max-glibc> <path>...}"
shift

version_gt() {
  [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" = "$1" ]
}

# Basenames that must come from the host, never from the AppImage / package tree.
is_forbidden_basename() {
  case "$1" in
    ld-linux*.so* | \
    libc.so.6 | libc-*.so | libm.so.6 | libmvec.so.1 | \
    libpthread.so.0 | libdl.so.2 | librt.so.1 | libresolv.so.2 | \
    libutil.so.1 | libanl.so.1 | libthread_db.so.1 | \
    libstdc++.so.6 | libgcc_s.so.1 | \
    libGL.so.* | libEGL.so.* | libGLESv*.so.* | libOpenGL.so.* | \
    libGLdispatch.so.* | libGLX.so.* | libglapi.so.* | libgbm.so.* | \
    libdrm.so.* | libdrm_*.so.* | \
    libX11.so.* | libX11-xcb.so.* | libxcb.so.* | \
    libwayland-client.so.* | libwayland-server.so.* | libwayland-egl.so.* | \
    libwayland-cursor.so.*)
      return 0
      ;;
  esac
  return 1
}

status=0
highest=0
while IFS= read -r -d '' file; do
  head -c 4 "$file" 2>/dev/null | grep -q $'^\x7fELF' || continue
  base="$(basename "$file")"
  if is_forbidden_basename "$base"; then
    echo "error: bundled host-only library: $file" >&2
    status=1
    continue
  fi
  need="$(objdump -T "$file" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -n1 || true)"
  [ -n "$need" ] || continue
  if version_gt "$need" "$highest"; then
    highest="$need"
  fi
  if version_gt "$need" "$max"; then
    echo "error: $file needs GLIBC_$need (> $max)" >&2
    status=1
  fi
done < <(find "$@" -type f -print0)

echo "highest glibc symbol version required: GLIBC_$highest (limit $max)"
exit "$status"
