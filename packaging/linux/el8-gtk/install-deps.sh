#!/usr/bin/env bash
# Host packages for the EL8 GTK toolchain. Kept separate from the compile
# script so a source-version bump does not reinstall the whole distro set.
set -euo pipefail

if [ -n "${ALMALINUX_MIRROR:-}" ]; then
  sed -i -e 's|^mirrorlist=|#mirrorlist=|' \
    -e "s|^# *baseurl=https://repo.almalinux.org/almalinux|baseurl=${ALMALINUX_MIRROR}|" \
    /etc/yum.repos.d/almalinux-*.repo
fi

dnf -y install dnf-plugins-core
dnf config-manager --set-enabled powertools >/dev/null 2>&1 \
  || dnf config-manager --set-enabled crb >/dev/null 2>&1 \
  || true

# No pcre2-devel: EL8's 10.32 is too old for GLib 2.78, and a second .pc file
# would be easy to pick up by accident. The toolchain builds pcre2 itself.
dnf -y --setopt=install_weak_deps=False install \
  autoconf \
  automake \
  binutils \
  bzip2 \
  brotli-devel \
  bzip2-devel \
  ca-certificates \
  curl \
  dbus-devel \
  diffutils \
  expat-devel \
  file \
  findutils \
  gcc \
  gcc-c++ \
  gcc-toolset-10 \
  gcc-toolset-10-gcc-c++ \
  gettext \
  gperf \
  gzip \
  itstool \
  libX11-devel \
  libXcursor-devel \
  libXdamage-devel \
  libXext-devel \
  libXfixes-devel \
  libXi-devel \
  libXinerama-devel \
  libXrandr-devel \
  libXrender-devel \
  libcurl-devel \
  libdrm-devel \
  libffi-devel \
  libglvnd-devel \
  libjpeg-turbo-devel \
  libmount-devel \
  libpng-devel \
  libtiff-devel \
  libuuid-devel \
  libxcb-devel \
  libxkbcommon-devel \
  libxml2-devel \
  libyaml-devel \
  make \
  openssl-devel \
  mesa-libEGL-devel \
  mesa-libGL-devel \
  patch \
  pkgconf-pkg-config \
  python3.11 \
  tar \
  unzip \
  xz \
  zlib-devel
dnf clean all
