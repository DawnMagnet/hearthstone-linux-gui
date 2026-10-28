# Native packages (deb, rpm, pacman) are built on AlmaLinux 9 (glibc 2.34).
# The AppImage is built on AlmaLinux 8 (glibc 2.28) against a GTK 4.12 /
# libadwaita 1.4 stack compiled from source in the el8-gtk-build stage, so it
# runs on Debian 11 and Ubuntu 20.04. glibc is never bundled.
#
# The toolchain stage does not copy the launcher sources. Publish it (see
# .github/workflows/packaging.yml) and reuse it with:
#   docker build --build-arg EL8_GTK_IMAGE=ghcr.io/<owner>/<repo>/el8-gtk:<tag>
# A local build with no build-arg compiles the stage in this Dockerfile.
#
#   docker build -f packaging/linux/Dockerfile --target dist \
#     --output type=local,dest=dist .
#   # AppImage only:
#   docker build -f packaging/linux/Dockerfile --target appimage-out \
#     --output type=local,dest=dist .
#
# Set ALMALINUX_MIRROR (e.g. https://ftp.osuosl.org/pub/almalinux) when the
# default mirror list is slow.

ARG BASE_IMAGE=docker.io/library/almalinux:9
# Declared before the first FROM so a later stage can use it. Default is the
# el8-gtk-build stage defined below; CI passes a published image name.
ARG EL8_GTK_IMAGE=el8-gtk-build

FROM ${BASE_IMAGE} AS toolchain

ARG RUST_VERSION=1.92.0
ARG NFPM_VERSION=2.43.1
ARG ALMALINUX_MIRROR=

ENV CARGO_HOME=/opt/cargo \
  RUSTUP_HOME=/opt/rustup \
  PATH=/opt/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Hearthstone's Unity player is x86_64-only; refuse other arches early.
RUN case "$(uname -m)" in x86_64) ;; *) echo "x86_64 only, got $(uname -m)" >&2; exit 1 ;; esac

RUN set -eux; \
  if [ -n "$ALMALINUX_MIRROR" ]; then \
    sed -i -e 's|^mirrorlist=|#mirrorlist=|' \
      -e "s|^# *baseurl=https://repo.almalinux.org/almalinux|baseurl=${ALMALINUX_MIRROR}|" \
      /etc/yum.repos.d/almalinux-*.repo; \
  fi; \
  dnf -y --setopt=install_weak_deps=False --enablerepo=crb install \
    binutils \
    dconf \
    desktop-file-utils \
    file \
    findutils \
    gcc \
    gdk-pixbuf2 \
    glib2-devel \
    gtk4-devel \
    ibus-gtk4 \
    libadwaita-devel \
    librsvg2 \
    librsvg2-tools \
    make \
    pkgconf-pkg-config \
    xz; \
  dnf clean all; \
  rpm -q glibc gtk4 libadwaita glib2

RUN set -eux; \
  curl -fsSL https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" \
      --default-host x86_64-unknown-linux-gnu; \
  curl -fsSL "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz" \
    | tar -xz -C /usr/local/bin nfpm; \
  chmod 755 /usr/local/bin/nfpm

FROM toolchain AS build

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY assets ./assets
COPY crates ./crates
COPY data ./data
COPY locales ./locales
COPY packaging ./packaging
COPY src ./src
COPY tests ./tests

RUN set -eux; \
  cargo build --workspace --release --locked; \
  cargo test --workspace --release --locked; \
  packaging/linux/stage.sh /pkgroot; \
  packaging/linux/check-glibc.sh 2.34 /pkgroot

FROM build AS packages
RUN packaging/linux/packages.sh /pkgroot /dist

# GTK stack for the AppImage. Versions and checksums live in el8-gtk/versions.env.
# The base is pinned to the almalinux:8 multi-arch index digest so the AppImage
# and the GHCR toolchain tag (a hash of this file) cannot drift with the tag.
# Bump it on purpose: update the digest and the date below together.
# almalinux:8 index digest as of 2026-10-28.
FROM docker.io/library/almalinux:8@sha256:2d0f2eb2a83cc09dbc2eb92fade4dbe8f709628149e81aa00aec70630406603b AS el8-gtk-build

ARG RUST_VERSION=1.92.0
ARG ALMALINUX_MIRROR=
ARG JOBS=

ENV ALMALINUX_MIRROR=${ALMALINUX_MIRROR} \
    RUST_VERSION=${RUST_VERSION} \
    JOBS=${JOBS} \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    PKG_CONFIG_PATH=/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig \
    MAX_GLIBC=2.28

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN case "$(uname -m)" in x86_64) ;; *) echo "x86_64 only, got $(uname -m)" >&2; exit 1 ;; esac

COPY packaging/linux/el8-gtk/install-deps.sh /opt/el8-gtk/install-deps.sh
RUN /opt/el8-gtk/install-deps.sh
COPY packaging/linux/el8-gtk/ /opt/el8-gtk/
RUN /opt/el8-gtk/build-toolchain.sh

# Reuse a published toolchain image by setting EL8_GTK_IMAGE. The default
# builds the stage above. An external name skips that stage.
FROM ${EL8_GTK_IMAGE} AS appimage

WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY assets ./assets
COPY crates ./crates
COPY data ./data
COPY locales ./locales
COPY packaging ./packaging
COPY src ./src

ENV MAX_GLIBC=2.28 \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    PKG_CONFIG_PATH=/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig

RUN set -eux; \
  cargo build --workspace --release --locked; \
  packaging/linux/stage.sh /pkgroot; \
  packaging/linux/appimage.sh /pkgroot /dist

FROM scratch AS dist
COPY --from=packages /dist /
COPY --from=appimage /dist /

# Clean export of just the AppImage (use: --target appimage-out --output type=local,dest=dist)
FROM scratch AS appimage-out
COPY --from=appimage /dist /

