#!/usr/bin/env bash
# Builds the AppImage from a staged tree linked on AlmaLinux 8 (glibc 2.28).
#
# - linuxdeploy copies the libraries the launcher needs and sets RUNPATH.
#   glibc, ld-linux, libstdc++, libgcc_s, GL/EGL, libdrm, libgbm and X11/xcb
#   stay on the host.
# - Wayland client libraries and the font stack are bundled. Ubuntu 20.04
#   ships Wayland 1.18 and GTK 4.12 needs >= 1.21; its HarfBuzz, FreeType
#   and fontconfig are older than the copies this GTK was built against.
#   linuxdeploy's built-in excludelist skips them, so they are passed with
#   --library, which force-deploys a library without pulling glibc along.
# - fontconfig's cache format is not stable across releases. The bundled
#   library loads $ORIGIN/../etc/fonts/fonts.conf (see
#   el8-gtk/fontconfig-bundle-config.patch), which uses a private cachedir
#   and does not read or write the host cache. No environment variable is
#   set, so child processes keep the host fontconfig.
# - GTK's runtime pieces (gdk-pixbuf loaders, the dconf GIO module, the ibus
#   input-method module, GSettings schemas) are added explicitly so the
#   bundled GTK never loads host modules built against a different GTK/GLib.
# - Tools are pinned to tagged releases and verified by checksum.
set -euo pipefail

root="${1:?usage: appimage.sh <pkgroot> <outdir>}"
out="${2:?usage: appimage.sh <pkgroot> <outdir>}"
max_glibc="${MAX_GLIBC:-2.28}"

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

# The toolchain curl has no TLS, and it is the libcurl.so.4 on the default
# linker path, so the distro curl binary cannot do https either. Python's
# ssl module uses OpenSSL directly.
download() {
  local url="$1" dest="$2" attempt
  for attempt in 1 2 3; do
    if python3.11 -c 'import sys, urllib.request; urllib.request.urlretrieve(sys.argv[1], sys.argv[2])' "$url" "$dest"; then
      return 0
    fi
    echo "download attempt ${attempt} failed: $url" >&2
    sleep 2
  done
  echo "error: download failed: $url" >&2
  return 1
}

fetch() {
  local url="$1" sha="$2" dest="$3"
  download "$url" "$dest"
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

# GSettings schemas used by the bundled GTK 4 and libadwaita. The EL8
# toolchain installs them under /usr/local; a distro GTK uses /usr.
schemas="$appdir/usr/share/glib-2.0/schemas"
mkdir -p "$schemas"
copied_schema=0
for schema_dir in /usr/local/share/glib-2.0/schemas /usr/share/glib-2.0/schemas; do
  [ -d "$schema_dir" ] || continue
  for xml in "$schema_dir"/org.gtk.gtk4.*.xml "$schema_dir"/org.gnome.Adwaita*.xml; do
    [ -f "$xml" ] || continue
    cp "$xml" "$schemas/"
    copied_schema=1
  done
done
[ "$copied_schema" = 1 ]
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
  --exclude-library 'libxcb.so*'
  --exclude-library 'libssl.so*'
  --exclude-library 'libcrypto.so*'
  --exclude-library 'libvulkan.so*'
  --exclude-library 'libwayland-server.so*'
  --exclude-library 'libwayland-client.so*'
  --exclude-library 'libwayland-cursor.so*'
  --exclude-library 'libwayland-egl.so*'
)

# These are on linuxdeploy's built-in excludelist (or are too old on the
# glibc 2.28 hosts we care about). --library force-deploys them and their
# non-excluded dependencies. Wayland server, GL, X11 and glibc stay out.
resolve_lib() {
  local name="$1" src
  src="$(ldconfig -p 2>/dev/null | awk -v n="$name" '$1 == n { print $NF; exit }')"
  if [ -z "$src" ] || [ ! -e "$src" ]; then
    echo "error: required library $name not found on the build host" >&2
    return 1
  fi
  # Keep the SONAME symlink. Canonicalizing makes linuxdeploy store
  # libfoo.so.0.23.1 and the loader then cannot find libfoo.so.0.
  printf '%s\n' "$src"
}

# linuxdeploy sometimes stores the real file and drops the SONAME link.
link_sonames() {
  local so soname
  for so in "$appdir/usr/lib"/*.so.*; do
    [ -f "$so" ] || continue
    [ -L "$so" ] && continue
    soname="$(readelf -d "$so" 2>/dev/null | awk -F'[][]' '/SONAME/ { print $2; exit }')"
    [ -n "$soname" ] || continue
    if [ ! -e "$appdir/usr/lib/$soname" ]; then
      ln -s "$(basename "$so")" "$appdir/usr/lib/$soname"
      echo "soname link: $soname -> $(basename "$so")"
    fi
  done
}
forced=()
for name in \
  libharfbuzz.so.0 \
  libfreetype.so.6 \
  libfontconfig.so.1 \
  libfribidi.so.0
do
  forced+=(--library "$(resolve_lib "$name")")
done

"$tools/linuxdeploy" \
  --appdir "$appdir" \
  --executable "$appdir/usr/bin/hearthstone-linux-gui" \
  --desktop-file "$appdir/usr/share/applications/$app_id.desktop" \
  --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/$app_id.png" \
  --custom-apprun packaging/appimage/AppRun \
  "${excludes[@]}" \
  "${deps_only[@]}" \
  "${forced[@]}"

# fontconfig finds this via $ORIGIN/../etc next to the bundled library.
# See el8-gtk/fonts.conf.
font_conf=""
for candidate in /usr/local/etc/fonts/fonts.conf /etc/fonts/fonts.conf; do
  if [ -f "$candidate" ] && grep -q 'hearthstone-linux-gui/fontconfig' "$candidate"; then
    font_conf="$candidate"
    break
  fi
done
if [ -z "$font_conf" ]; then
  echo "error: private fontconfig config was not installed" >&2
  exit 1
fi
mkdir -p "$appdir/usr/etc/fonts"
cp "$font_conf" "$appdir/usr/etc/fonts/fonts.conf"
link_sonames

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
if grep -R -a -q -F 'libgpg-error.so.0' "$appdir/usr" 2>/dev/null; then
  copy_host_lib libgpg-error.so.0
fi

for bundled in \
  libharfbuzz.so.0 \
  libfreetype.so.6 \
  libfontconfig.so.1 \
  libfribidi.so.0
do
  found="$(find "$appdir/usr/lib" -maxdepth 1 -name "$bundled" -print -quit)"
  if [ -z "$found" ]; then
    echo "error: $bundled was not bundled into the AppImage" >&2
    exit 1
  fi
done

# Wayland client/cursor are NOT on the default library path. GTK 4.12 imports
# wl_proxy_marshal_flags (Wayland 1.21); Debian 11 and Ubuntu 20.04 export
# everything else we use but not that symbol. AppRun prepends this directory
# to LD_LIBRARY_PATH only when the host library's filename says it is older
# than 1.21. libwayland-egl is intentionally absent: it must be the host's,
# paired with host Mesa.
compat="$appdir/usr/lib/wayland-compat"
mkdir -p "$compat"
wayland_ver="$(pkg-config --modversion wayland-client)"
printf '%s\n' "$wayland_ver" > "$compat/VERSION"
for name in libwayland-client.so.0 libwayland-cursor.so.0; do
  src="$(resolve_lib "$name")"
  real="$(readlink -f "$src")"
  cp -a "$real" "$compat/"
  ln -sfn "$(basename "$real")" "$compat/$name"
done
# Built on EL8 against libffi.so.6. Debian 11 and Ubuntu 20.04 ship
# libffi.so.7, so the compat copies need the EL8 libffi beside them.
ffi_real="$(readlink -f "$(resolve_lib libffi.so.6)")"
cp -a "$ffi_real" "$compat/"
ln -sfn "$(basename "$ffi_real")" "$compat/libffi.so.6"
PATCHELF_VERSION=0.18.0
PATCHELF_URL=https://github.com/NixOS/patchelf/releases/download/${PATCHELF_VERSION}/patchelf-${PATCHELF_VERSION}-x86_64.tar.gz
PATCHELF_SHA256=ce84f2447fb7a8679e58bc54a20dc2b01b37b5802e12c57eece772a6f14bf3f0
patchelf_tar="$work/patchelf.tar.gz"
download "$PATCHELF_URL" "$patchelf_tar"
echo "$PATCHELF_SHA256  $patchelf_tar" | sha256sum -c -
tar -xzf "$patchelf_tar" -C "$work" ./bin/patchelf
for so in "$compat"/libwayland-*.so.*; do
  [ -L "$so" ] && continue
  # $ORIGIN is a literal dynamic-loader token, not a shell variable.
  # shellcheck disable=SC2016
  "$work/bin/patchelf" --set-rpath '$ORIGIN' "$so"
done
if [ -e "$appdir/usr/lib/libwayland-egl.so.1" ] || [ -e "$compat/libwayland-egl.so.1" ]; then
  echo "error: libwayland-egl must stay on the host" >&2
  exit 1
fi

# Exactly the names in appimage-icons.txt, as SVGs from adwaita-icon-theme
# (pinned in el8-gtk/versions.env). They go in hicolor, the fallback theme,
# not a bundled Adwaita theme. GTK and libadwaita paint their own widget
# icons from resources. AppRun prepends $APPDIR/usr/share to XDG_DATA_DIRS
# and host_env restores the original value for children.
# shellcheck disable=SC1091
. packaging/linux/el8-gtk/versions.env
icons_tar="$work/adwaita-icon-theme-${ADWAITA_ICON_THEME_VERSION}.tar.xz"
download "$ADWAITA_ICON_THEME_URL" "$icons_tar"
echo "$ADWAITA_ICON_THEME_SHA256  $icons_tar" | sha256sum -c -
icons_src="$work/adwaita-icon-theme"
mkdir -p "$icons_src"
tar -xJf "$icons_tar" -C "$icons_src" --strip-components=1
icon_dir="$appdir/usr/share/icons/hicolor/scalable/actions"
mkdir -p "$icon_dir" "$appdir/usr/share/licenses/adwaita-icon-theme"
install -m 644 "$icons_src/COPYING_CCBYSA3" "$icons_src/COPYING_LGPL" \
  "$appdir/usr/share/licenses/adwaita-icon-theme/"
while IFS= read -r icon_name || [ -n "$icon_name" ]; do
  case "$icon_name" in
    ''|\#*) continue ;;
  esac
  icon_name="${icon_name%%.svg}"
  found="$(find "$icons_src/Adwaita/symbolic" -name "${icon_name}.svg" -print -quit)"
  if [ -z "$found" ]; then
    echo "error: ${icon_name}.svg not in adwaita-icon-theme ${ADWAITA_ICON_THEME_VERSION}" >&2
    exit 1
  fi
  install -m 644 "$found" "$icon_dir/${icon_name}.svg"
done < packaging/linux/appimage-icons.txt
cat > "$appdir/usr/share/icons/hicolor/index.theme" <<'EOF'
[Icon Theme]
Name=Hicolor
Comment=Fallback icon theme
Directories=scalable/actions

[scalable/actions]
Context=Actions
Size=128
MinSize=1
MaxSize=256
Type=Scalable
EOF
while IFS= read -r icon_name || [ -n "$icon_name" ]; do
  case "$icon_name" in
    ''|\#*) continue ;;
  esac
  icon_name="${icon_name%%.svg}"
  if [ ! -f "$icon_dir/${icon_name}.svg" ]; then
    echo "error: AppDir is missing usr/share/icons/hicolor/scalable/actions/${icon_name}.svg" >&2
    exit 1
  fi
done < packaging/linux/appimage-icons.txt

# Runtime pieces GTK finds without guessing. Paths are inside the AppDir;
# AppRun rewrites the pixbuf template and points the module variables here,
# then host_env clears those variables for children.
loaders_in="$appdir/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache.in"
grep -q '@APPDIR@/' "$loaders_in"
grep -q 'libpixbufloader-svg.so' "$loaders_in"
[ -f "$appdir/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so" ]
[ -f "$appdir/usr/lib/gio/modules/libdconfsettings.so" ]
[ -f "$appdir/usr/share/glib-2.0/schemas/gschemas.compiled" ]
[ -f "$appdir/usr/etc/fonts/fonts.conf" ]

# Belt and braces: nothing from the excludelist may have slipped in, and no
# bundled file may need a newer glibc than the baseline.
packaging/linux/check-glibc.sh "$max_glibc" "$appdir"

# libstdc++ and libgcc_s stay on the host. Ubuntu 20.04 / Debian 11 provide
# GLIBCXX through 3.4.28, CXXABI through 1.3.12 and libgcc_s through GCC_7.0.0
# (gcc-toolset-10's ceiling). A newer symbol would load and then fail there.
host_cxx_ok() {
  local need="$1" max="$2"
  [ "$need" = "$max" ] && return 0
  [ "$(printf '%s\n%s\n' "$need" "$max" | sort -V | tail -n1)" = "$max" ]
}
cxx_bad=0
while IFS= read -r -d '' file; do
  head -c 4 "$file" 2>/dev/null | grep -q $'^\x7fELF' || continue
  while read -r sym; do
    case "$sym" in
      GLIBCXX_*)
        ver="${sym#GLIBCXX_}"
        host_cxx_ok "$ver" 3.4.28 || { echo "error: $file needs $sym" >&2; cxx_bad=1; }
        ;;
      CXXABI_*)
        ver="${sym#CXXABI_}"
        host_cxx_ok "$ver" 1.3.12 || { echo "error: $file needs $sym" >&2; cxx_bad=1; }
        ;;
      GCC_*)
        ver="${sym#GCC_}"
        host_cxx_ok "$ver" 7.0.0 || { echo "error: $file needs $sym" >&2; cxx_bad=1; }
        ;;
    esac
  done < <(objdump -T "$file" 2>/dev/null | grep -oE '(GLIBCXX|CXXABI|GCC)_[0-9.]+' | sort -u || true)
done < <(find "$appdir" -type f -print0)
[ "$cxx_bad" = 0 ]

target="$out/hearthstone-linux-gui-${version}-x86_64.AppImage"
ARCH=x86_64 "$tools/appimagetool" --no-appstream --runtime-file "$tools/runtime-x86_64" "$appdir" "$target"
chmod 755 "$target"

# Smoke test the finished image on the build baseline.
"$target" --no-gui
