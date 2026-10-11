#!/usr/bin/env bash
# Builds the AppImage from a staged tree on the Enterprise Linux 9 baseline.
#
# - linuxdeploy copies the libraries the launcher needs and sets RUNPATH; its
#   built-in excludelist keeps glibc, ld-linux, libstdc++, libgcc_s, GL/EGL,
#   libdrm, X11/xcb core, libwayland-client, fontconfig/freetype/harfbuzz on
#   the host.
# - GTK's runtime pieces (gdk-pixbuf loaders, the dconf GIO module, the ibus
#   input-method module, GSettings schemas) are added explicitly so the
#   bundled GTK never loads host modules built against a different GTK/GLib.
# - Tools are pinned to tagged releases and verified by checksum.
set -euo pipefail

root="${1:?usage: appimage.sh <pkgroot> <outdir>}"
out="${2:?usage: appimage.sh <pkgroot> <outdir>}"
max_glibc="${MAX_GLIBC:-2.34}"

LINUXDEPLOY_VERSION=1-alpha-20251107-1
LINUXDEPLOY_SHA256=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
APPIMAGETOOL_VERSION=1.9.1
APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
TYPE2_RUNTIME_VERSION=20251108
TYPE2_RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

app_id=io.github.hearthstone_linux_gui
version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
work="$(mktemp -d)"
tools="$work/tools"
appdir="$work/AppDir"
mkdir -p "$tools" "$out"

# The tools are AppImages themselves; there is no FUSE in a container.
export APPIMAGE_EXTRACT_AND_RUN=1

fetch() {
  local url="$1" sha="$2" dest="$3"
  curl -fsSL --retry 3 -o "$dest" "$url"
  echo "$sha  $dest" | sha256sum -c -
  chmod 755 "$dest"
}

fetch "https://github.com/linuxdeploy/linuxdeploy/releases/download/${LINUXDEPLOY_VERSION}/linuxdeploy-x86_64.AppImage" \
  "$LINUXDEPLOY_SHA256" "$tools/linuxdeploy"
fetch "https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage" \
  "$APPIMAGETOOL_SHA256" "$tools/appimagetool"
fetch "https://github.com/AppImage/type2-runtime/releases/download/${TYPE2_RUNTIME_VERSION}/runtime-x86_64" \
  "$TYPE2_RUNTIME_SHA256" "$tools/runtime-x86_64"

mkdir -p "$appdir"
cp -a "$root/usr" "$appdir/usr"

# gdk-pixbuf loaders (SVG icons through librsvg, plus whatever is modular).
pixbuf_moduledir="$(pkg-config --variable=gdk_pixbuf_moduledir gdk-pixbuf-2.0)"
pixbuf_dir="$appdir/usr/lib/gdk-pixbuf-2.0/2.10.0"
mkdir -p "$pixbuf_dir/loaders"
cp -L "$pixbuf_moduledir"/*.so "$pixbuf_dir/loaders/"
query_loaders="$(command -v gdk-pixbuf-query-loaders-64 || command -v gdk-pixbuf-query-loaders)"
"$query_loaders" "$pixbuf_dir"/loaders/*.so \
  | sed -e "s|$appdir|@APPDIR@|g" -e '/^#/d' > "$pixbuf_dir/loaders.cache.in"
grep -q svg "$pixbuf_dir/loaders.cache.in"

# GIO modules: only dconf, so GSettings reads the user's real settings.
gio_moduledir="$(pkg-config --variable=giomoduledir gio-2.0)"
mkdir -p "$appdir/usr/lib/gio/modules"
if [ -f "$gio_moduledir/libdconfsettings.so" ]; then
  cp -L "$gio_moduledir/libdconfsettings.so" "$appdir/usr/lib/gio/modules/"
fi

# GTK input-method modules: ibus (fcitx5 also speaks the ibus protocol).
# On Wayland GTK's built-in text-input support is used instead.
gtk_libdir="$(pkg-config --variable=libdir gtk4)/gtk-4.0"
gtk_binary_version="$(pkg-config --variable=gtk_binary_version gtk4)"
if [ -d "$gtk_libdir/$gtk_binary_version/immodules" ]; then
  mkdir -p "$appdir/usr/lib/gtk-4.0/$gtk_binary_version"
  cp -aL "$gtk_libdir/$gtk_binary_version/immodules" "$appdir/usr/lib/gtk-4.0/$gtk_binary_version/"
fi

# GSettings schemas used by GTK 4 itself.
schemas="$appdir/usr/share/glib-2.0/schemas"
mkdir -p "$schemas"
cp /usr/share/glib-2.0/schemas/org.gtk.gtk4.*.xml "$schemas/"
glib-compile-schemas "$schemas"

deps_only=()
for dir in "$pixbuf_dir/loaders" "$appdir/usr/lib/gio/modules" "$appdir/usr/lib/gtk-4.0"; do
  [ -d "$dir" ] || continue
  while IFS= read -r -d '' module; do
    deps_only+=("--deploy-deps-only=$module")
  done < <(find "$dir" -name '*.so' -print0 2>/dev/null)
done

# Explicit excludes (belt-and-braces on top of linuxdeploy's built-in
# excludelist). Do not use the linuxdeploy GTK plugin: it forces
# GDK_BACKEND=x11 / GTK_THEME and its AppRun hook would not record host values.
excludes=(
  --exclude-library 'ld-linux*.so*'
  --exclude-library 'libc.so.6'
  --exclude-library 'libm.so.6'
  --exclude-library 'libpthread.so.0'
  --exclude-library 'libdl.so.2'
  --exclude-library 'librt.so.1'
  --exclude-library 'libstdc++.so.6'
  --exclude-library 'libgcc_s.so.1'
  --exclude-library 'libGL.so*'
  --exclude-library 'libEGL.so*'
  --exclude-library 'libOpenGL.so*'
  --exclude-library 'libGLdispatch.so*'
  --exclude-library 'libGLX.so*'
  --exclude-library 'libdrm.so*'
  --exclude-library 'libgbm.so*'
  --exclude-library 'libX11.so*'
  --exclude-library 'libX11-xcb.so*'
  --exclude-library 'libxcb.so.1'
  --exclude-library 'libwayland-client.so*'
  --exclude-library 'libwayland-server.so*'
  --exclude-library 'libwayland-egl.so*'
  --exclude-library 'libwayland-cursor.so*'
)

"$tools/linuxdeploy" \
  --appdir "$appdir" \
  --executable "$appdir/usr/bin/hearthstone-linux-gui" \
  --desktop-file "$appdir/usr/share/applications/$app_id.desktop" \
  --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/$app_id.png" \
  --custom-apprun packaging/appimage/AppRun \
  "${excludes[@]}" \
  "${deps_only[@]}"

# linuxdeploy's default exclude list omits libgpg-error even though bundled
# libgcrypt / libsystemd need it. Copy it explicitly so hosts without the
# package (e.g. Debian images with Install-Recommends disabled) still resolve.
copy_host_lib() {
  local name="$1" src
  [ -e "$appdir/usr/lib/$name" ] && return 0
  src="$(ldconfig -p 2>/dev/null | awk -v n="$name" '$1 == n { print $NF; exit }')"
  if [ -z "$src" ] || [ ! -f "$src" ]; then
    echo "error: required library $name not found on the build host" >&2
    return 1
  fi
  cp -L "$src" "$appdir/usr/lib/"
  echo "bundled host library: $name <- $src"
}
copy_host_lib libgpg-error.so.0

# Belt and braces: nothing from the excludelist may have slipped in, and no
# bundled file may need a newer glibc than the baseline.
packaging/linux/check-glibc.sh "$max_glibc" "$appdir"

target="$out/hearthstone-linux-gui-${version}-x86_64.AppImage"
ARCH=x86_64 "$tools/appimagetool" --no-appstream --runtime-file "$tools/runtime-x86_64" "$appdir" "$target"
chmod 755 "$target"

# Smoke test the finished image on the build baseline.
"$target" --no-gui
