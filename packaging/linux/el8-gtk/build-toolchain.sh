#!/usr/bin/env bash
# Builds the GTK 4.12 / libadwaita 1.4 stack from the versions in versions.env
# and installs it under /usr/local. Run inside the el8-gtk-build image.
#
# The compiler is gcc-toolset-10 (GCC 10). That is new enough for this stack
# and its libstdc++ stops at GLIBCXX_3.4.28, which Ubuntu 20.04 and Debian 11
# both provide. A newer toolset would emit GLIBCXX_3.4.29+ and the AppImage
# would not start on those hosts, because libstdc++ stays on the host.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
. "$here/versions.env"

prefix=/usr/local
jobs="${JOBS:-$(nproc)}"
src=/tmp/el8-src
export CARGO_HOME=/opt/cargo
export RUSTUP_HOME=/opt/rustup
export PATH="/opt/cargo/bin:${prefix}/bin:${PATH}"
# wayland-protocols installs its .pc file under share/pkgconfig, not lib/pkgconfig.
export PKG_CONFIG_PATH="${prefix}/lib/pkgconfig:${prefix}/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export CARGO_BUILD_JOBS="$jobs"

mkdir -p "$src"
echo /usr/local/lib >/etc/ld.so.conf.d/00-usr-local.conf

# The toolset enable script reads MANPATH, which `set -u` rejects when unset.
set +u
# shellcheck disable=SC1091
. /opt/rh/gcc-toolset-10/enable
set -u
# Meson looks up `cc` before PATH, and /usr/bin/cc on EL8 is GCC 8.
# Point it at the toolset explicitly. GCC 10's libstdc++ stops at
# GLIBCXX_3.4.28, which Ubuntu 20.04 and Debian 11 both ship.
export CC=/opt/rh/gcc-toolset-10/root/usr/bin/gcc
export CXX=/opt/rh/gcc-toolset-10/root/usr/bin/g++
export PATH="/opt/cargo/bin:${prefix}/bin:${PATH}"

fetch() {
  local url="$1"
  local sha="$2"
  local name="$3"
  local dest="$src/$name"
  curl -fsSL --retry 5 --retry-delay 2 -o "$dest.partial" "$url"
  mv "$dest.partial" "$dest"
  echo "$sha  $dest" | sha256sum -c - >&2
  printf '%s\n' "$dest"
}

extract() {
  local archive="$1" dest="$2"
  rm -rf "$dest"
  mkdir -p "$dest"
  # EL8 tar prefers lbzip2, which is not installed. Use bzip2 directly.
  case "$archive" in
    *.bz2) bzip2 -dc "$archive" | tar -xf - -C "$dest" --strip-components=1 ;;
    *) tar -xf "$archive" -C "$dest" --strip-components=1 ;;
  esac
}

autotools_install() {
  local tree="$1"
  shift
  (
    cd "$tree"
    ./configure --prefix="$prefix" --libdir="$prefix/lib" --disable-static "$@"
    make -j "$jobs"
    make install
  )
  ldconfig
}

meson_install() {
  local tree="$1"
  shift
  rm -rf "$tree/build"
  meson setup "$tree/build" "$tree" \
    --prefix="$prefix" \
    --libdir=lib \
    --buildtype=release \
    -Ddefault_library=shared \
    "$@"
  meson compile -C "$tree/build" -j "$jobs"
  meson install -C "$tree/build"
  ldconfig
}

echo "== meson ${MESON_VERSION} / ninja ${NINJA_VERSION}"
extract "$(fetch "$MESON_URL" "$MESON_SHA256" "meson-${MESON_VERSION}.tar.gz")" /opt/meson
cat >/usr/local/bin/meson <<'EOF'
#!/bin/sh
exec python3.11 /opt/meson/meson.py "$@"
EOF
chmod 755 /usr/local/bin/meson
unzip -o "$(fetch "$NINJA_URL" "$NINJA_SHA256" "ninja-${NINJA_VERSION}.zip")" -d /usr/local/bin
chmod 755 /usr/local/bin/ninja
meson --version
ninja --version

echo "== rust ${RUST_VERSION:-1.92.0}"
curl -fsSL https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION:-1.92.0}" \
      --default-host x86_64-unknown-linux-gnu
rustc --version

echo "== pcre2 ${PCRE2_VERSION}"
extract "$(fetch "$PCRE2_URL" "$PCRE2_SHA256" "pcre2-${PCRE2_VERSION}.tar.bz2")" "$src/pcre2"
autotools_install "$src/pcre2" --enable-pcre2-16 --enable-pcre2-32
pkg-config --modversion libpcre2-8 | grep -qx "$PCRE2_VERSION"

echo "== glib ${GLIB_VERSION}"
extract "$(fetch "$GLIB_URL" "$GLIB_SHA256" "glib-${GLIB_VERSION}.tar.xz")" "$src/glib"
meson_install "$src/glib" \
  -Dtests=false \
  -Dinstalled_tests=false \
  -Dman=false \
  -Dgtk_doc=false \
  -Dsysprof=disabled \
  -Dselinux=disabled \
  -Dlibmount=enabled \
  -Dnls=enabled

echo "== libxmlb ${LIBXMLB_VERSION}"
extract "$(fetch "$LIBXMLB_URL" "$LIBXMLB_SHA256" "libxmlb-${LIBXMLB_VERSION}.tar.gz")" "$src/libxmlb"
meson_install "$src/libxmlb" \
  -Dgtkdoc=false \
  -Dintrospection=false \
  -Dtests=false \
  -Dcli=false \
  -Dlzma=disabled \
  -Dzstd=disabled

echo "== curl ${CURL_VERSION}"
extract "$(fetch "$CURL_URL" "$CURL_SHA256" "curl-${CURL_VERSION}.tar.xz")" "$src/curl"
autotools_install "$src/curl" \
  --with-openssl \
  --with-zlib \
  --without-libpsl \
  --without-libidn2 \
  --without-brotli \
  --without-zstd \
  --without-nghttp2 \
  --without-libssh2 \
  --without-gssapi \
  --without-librtmp \
  --without-nghttp3 \
  --without-ngtcp2 \
  --disable-ldap \
  --disable-ldaps
pkg-config --modversion libcurl

echo "== appstream ${APPSTREAM_VERSION}"
extract "$(fetch "$APPSTREAM_URL" "$APPSTREAM_SHA256" "appstream-${APPSTREAM_VERSION}.tar.gz")" "$src/appstream"
# Man pages are unconditional and need xsltproc plus the DocBook stylesheets.
# libadwaita only links the library.
sed -i "s|subdir('docs/')|# subdir('docs/')|" "$src/appstream/meson.build"
meson_install "$src/appstream" \
  -Dstemming=false \
  -Dsystemd=false \
  -Dgir=false \
  -Dsvg-support=false \
  -Dzstd-support=false \
  -Ddocs=false \
  -Dapidocs=false \
  -Dinstall-docs=false \
  -Dqt=false \
  -Dcompose=false \
  -Dapt-support=false \
  -Dvapi=false

echo "== fribidi ${FRIBIDI_VERSION}"
extract "$(fetch "$FRIBIDI_URL" "$FRIBIDI_SHA256" "fribidi-${FRIBIDI_VERSION}.tar.xz")" "$src/fribidi"
meson_install "$src/fribidi" \
  -Ddocs=false \
  -Dtests=false \
  -Dbin=false

echo "== pixman ${PIXMAN_VERSION}"
extract "$(fetch "$PIXMAN_URL" "$PIXMAN_SHA256" "pixman-${PIXMAN_VERSION}.tar.gz")" "$src/pixman"
meson_install "$src/pixman" \
  -Dtests=disabled \
  -Dgtk=disabled \
  -Dlibpng=disabled

echo "== freetype ${FREETYPE_VERSION}"
extract "$(fetch "$FREETYPE_URL" "$FREETYPE_SHA256" "freetype-${FREETYPE_VERSION}.tar.xz")" "$src/freetype"
meson_install "$src/freetype" \
  -Dharfbuzz=disabled \
  -Dbrotli=enabled \
  -Dbzip2=enabled \
  -Dpng=enabled \
  -Dzlib=enabled \
  -Dtests=disabled

echo "== fontconfig ${FONTCONFIG_VERSION}"
extract "$(fetch "$FONTCONFIG_URL" "$FONTCONFIG_SHA256" "fontconfig-${FONTCONFIG_VERSION}.tar.xz")" "$src/fontconfig"
patch -p1 -d "$src/fontconfig" < "$here/fontconfig-bundle-config.patch"
(
  # dladdr lives in libdl on glibc 2.28.
  export LDFLAGS="-Wl,--no-as-needed -ldl${LDFLAGS:+ $LDFLAGS}"
  meson_install "$src/fontconfig" \
    -Ddoc=disabled \
    -Dtests=disabled \
    -Dtools=enabled \
    -Dcache-build=disabled \
    -Dnls=disabled \
    -Diconv=enabled
)
install -Dm644 "$here/fonts.conf" "$prefix/etc/fonts/fonts.conf"
ldconfig
font_list="$(fc-conflist)"
printf '%s\n' "$font_list"
# dladdr reports the path with the libdir left in it ($ORIGIN/../etc/...).
if ! printf '%s\n' "$font_list" | grep -Eq '/usr/local/(lib/\.\./)?etc/fonts/fonts.conf'; then
  echo "error: bundled fontconfig did not load $prefix/etc/fonts/fonts.conf" >&2
  exit 1
fi
if printf '%s\n' "$font_list" | awk '{ print $2 }' | sed 's/:$//' | grep -qx '/etc/fonts/fonts.conf'; then
  echo "error: bundled fontconfig loaded the host fonts.conf" >&2
  exit 1
fi

echo "== harfbuzz ${HARFBUZZ_VERSION}"
extract "$(fetch "$HARFBUZZ_URL" "$HARFBUZZ_SHA256" "harfbuzz-${HARFBUZZ_VERSION}.tar.xz")" "$src/harfbuzz"
meson_install "$src/harfbuzz" \
  -Dtests=disabled \
  -Ddocs=disabled \
  -Dintrospection=disabled \
  -Dbenchmark=disabled \
  -Dutilities=disabled \
  -Dglib=enabled \
  -Dfreetype=enabled \
  -Dcairo=disabled \
  -Dicu=disabled \
  -Dgraphite2=disabled

echo "== cairo ${CAIRO_VERSION}"
extract "$(fetch "$CAIRO_URL" "$CAIRO_SHA256" "cairo-${CAIRO_VERSION}.tar.xz")" "$src/cairo"
meson_install "$src/cairo" \
  -Dtests=disabled \
  -Dgtk_doc=false \
  -Dsymbol-lookup=disabled \
  -Dspectre=disabled \
  -Dquartz=disabled \
  -Dxcb=disabled \
  -Dxlib=enabled \
  -Dxlib-xcb=disabled \
  -Dpng=enabled \
  -Dzlib=enabled \
  -Dfontconfig=enabled \
  -Dfreetype=enabled \
  -Dglib=enabled \
  -Dtee=disabled

echo "== pango ${PANGO_VERSION}"
extract "$(fetch "$PANGO_URL" "$PANGO_SHA256" "pango-${PANGO_VERSION}.tar.xz")" "$src/pango"
meson_install "$src/pango" \
  -Dgtk_doc=false \
  -Dintrospection=disabled \
  -Dinstall-tests=false \
  -Dsysprof=disabled \
  -Dlibthai=disabled \
  -Dcairo=enabled \
  -Dxft=disabled \
  -Dfreetype=enabled \
  -Dfontconfig=enabled

echo "== graphene ${GRAPHENE_VERSION}"
extract "$(fetch "$GRAPHENE_URL" "$GRAPHENE_SHA256" "graphene-${GRAPHENE_VERSION}.tar.xz")" "$src/graphene"
meson_install "$src/graphene" \
  -Dgtk_doc=false \
  -Dintrospection=disabled \
  -Dtests=false \
  -Dinstalled_tests=false \
  -Dgobject_types=true

echo "== libepoxy ${LIBEPOXY_VERSION}"
extract "$(fetch "$LIBEPOXY_URL" "$LIBEPOXY_SHA256" "libepoxy-${LIBEPOXY_VERSION}.tar.gz")" "$src/libepoxy"
meson_install "$src/libepoxy" \
  -Ddocs=false \
  -Dtests=false \
  -Dglx=yes \
  -Degl=yes \
  -Dx11=true

echo "== wayland ${WAYLAND_VERSION}"
extract "$(fetch "$WAYLAND_URL" "$WAYLAND_SHA256" "wayland-${WAYLAND_VERSION}.tar.xz")" "$src/wayland"
meson_install "$src/wayland" \
  -Ddocumentation=false \
  -Dtests=false \
  -Ddtd_validation=false \
  -Dlibraries=true \
  -Dscanner=true

echo "== wayland-protocols ${WAYLAND_PROTOCOLS_VERSION}"
extract "$(fetch "$WAYLAND_PROTOCOLS_URL" "$WAYLAND_PROTOCOLS_SHA256" "wayland-protocols-${WAYLAND_PROTOCOLS_VERSION}.tar.xz")" "$src/wayland-protocols"
meson_install "$src/wayland-protocols" -Dtests=false

echo "== gdk-pixbuf ${GDK_PIXBUF_VERSION}"
extract "$(fetch "$GDK_PIXBUF_URL" "$GDK_PIXBUF_SHA256" "gdk-pixbuf-${GDK_PIXBUF_VERSION}.tar.xz")" "$src/gdk-pixbuf"
meson_install "$src/gdk-pixbuf" \
  -Dpng=enabled \
  -Djpeg=enabled \
  -Dtiff=enabled \
  -Dgif=enabled \
  -Dothers=disabled \
  -Dbuiltin_loaders=png,jpeg \
  -Dgtk_doc=false \
  -Ddocs=false \
  -Dintrospection=disabled \
  -Dman=false \
  -Dtests=false \
  -Dinstalled_tests=false \
  -Dgio_sniffing=false

echo "== librsvg ${LIBRSVG_VERSION}"
extract "$(fetch "$LIBRSVG_URL" "$LIBRSVG_SHA256" "librsvg-${LIBRSVG_VERSION}.tar.xz")" "$src/librsvg"
(
  cd "$src/librsvg"
  ./configure \
    --prefix="$prefix" \
    --libdir="$prefix/lib" \
    --disable-static \
    --disable-gtk-doc \
    --disable-introspection \
    --enable-pixbuf-loader
  make -j "$jobs"
  make install
)
ldconfig
rm -rf "${CARGO_HOME}/registry" "${CARGO_HOME}/git"

echo "== dconf ${DCONF_VERSION}"
extract "$(fetch "$DCONF_URL" "$DCONF_SHA256" "dconf-${DCONF_VERSION}.tar.xz")" "$src/dconf"
meson_install "$src/dconf" \
  -Dbash_completion=false \
  -Dman=false \
  -Dgtk_doc=false \
  -Dvapi=false

echo "== gtk ${GTK_VERSION}"
extract "$(fetch "$GTK_URL" "$GTK_SHA256" "gtk-${GTK_VERSION}.tar.xz")" "$src/gtk"
# Vulkan stays off: GTK 4.12's GSK renderer is GL, and a Vulkan build would
# pull the loader into the AppImage. The GL renderer is what we ship.
meson_install "$src/gtk" \
  -Dintrospection=disabled \
  -Dbuild-tests=false \
  -Dbuild-examples=false \
  -Dbuild-demos=false \
  -Dbuild-testsuite=false \
  -Dmedia-gstreamer=disabled \
  -Dmedia-ffmpeg=disabled \
  -Dprint-cups=disabled \
  -Dprint-cpdb=disabled \
  -Dvulkan=disabled \
  -Dcloudproviders=disabled \
  -Dsysprof=disabled \
  -Dtracker=disabled \
  -Dcolord=disabled \
  -Ddocumentation=false \
  -Dgtk_doc=false \
  -Dman-pages=false \
  -Dx11-backend=true \
  -Dwayland-backend=true \
  -Dbroadway-backend=true

echo "== libadwaita ${LIBADWAITA_VERSION}"
extract "$(fetch "$LIBADWAITA_URL" "$LIBADWAITA_SHA256" "libadwaita-${LIBADWAITA_VERSION}.tar.xz")" "$src/libadwaita"
meson_install "$src/libadwaita" \
  -Dintrospection=disabled \
  -Dvapi=false \
  -Dtests=false \
  -Dexamples=false \
  -Dgtk_doc=false

rm -rf "$src"
ldconfig

echo "== versions"
pkg-config --modversion glib-2.0
pkg-config --modversion gtk4
pkg-config --modversion libadwaita-1
pkg-config --modversion wayland-client
pkg-config --modversion gtk4 | grep -qx '4.12.*'
pkg-config --modversion libadwaita-1 | grep -qx '1.4.*'
python3.11 - <<'PY'
import subprocess
ver = subprocess.check_output(["pkg-config", "--modversion", "wayland-client"], text=True).strip()
parts = [int(x) for x in ver.split(".")[:3]]
if parts < [1, 21, 0]:
    raise SystemExit(f"wayland-client {ver} is older than 1.21")
print("wayland-client", ver, "ok")
PY

while IFS= read -r -d '' so; do
  if readelf -d "$so" 2>/dev/null | grep -E 'RPATH|RUNPATH' | grep -q '/opt/rh'; then
    echo "error: $so is linked with a gcc-toolset RPATH" >&2
    exit 1
  fi
done < <(find "$prefix/lib" -type f -name '*.so*' -print0)

echo "== el8 gtk toolchain ready"
