#!/usr/bin/env bash
# Fails when any ELF file under the given paths needs a glibc symbol version
# newer than <max>, or when a host library that must never be bundled is
# present. That list is glibc and the dynamic loader, libstdc++, libgcc_s,
# OpenSSL, GL/EGL/GLX/GLdispatch, libgbm, libdrm, Vulkan, Mesa drivers,
# libX11, libxcb, the Wayland server library, and libwayland-egl (it has to
# match host Mesa).
#
# libwayland-client and libwayland-cursor may exist only under a
# wayland-compat directory. AppRun puts that directory on LD_LIBRARY_PATH
# when the host client is older than Wayland 1.21, and host_env takes it
# off again before spawning children. Anywhere else they are an error:
# RUNPATH would make the GUI load them even on a new host.
#
#   check-glibc.sh 2.28 /pkgroot AppDir
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
    libssl.so.* | libcrypto.so.* | \
    libGL.so.* | libEGL.so.* | libGLESv*.so.* | libOpenGL.so.* | \
    libGLdispatch.so.* | libGLX.so.* | libGLX_mesa.so.* | libEGL_mesa.so.* | \
    libglapi.so.* | libgbm.so.* | libgallium*.so* | *_dri.so | \
    libdrm.so.* | libdrm_*.so.* | \
    libvulkan.so.* | libvulkan_*.so.* | \
    libX11.so.* | libX11-xcb.so.* | libxcb.so.* | libxcb-*.so.* | \
    libwayland-server.so.* | libwayland-egl.so.*)
      return 0
      ;;
  esac
  return 1
}

# 0 when this file must not be in the tree.
is_forbidden_path() {
  local path="$1" base
  base="$(basename "$path")"
  case "$base" in
    libwayland-client.so.* | libwayland-cursor.so.*)
      case "$path" in
        */wayland-compat/*) return 1 ;;
        *) return 0 ;;
      esac
      ;;
  esac
  is_forbidden_basename "$base"
}

status=0
highest=0
while IFS= read -r -d '' file; do
  head -c 4 "$file" 2>/dev/null | grep -q $'^\x7fELF' || continue
  base="$(basename "$file")"
  if is_forbidden_path "$file"; then
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
