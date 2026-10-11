# Builds every non-Nix release artifact (deb, rpm, pacman, AppImage) on the
# oldest still-supported Linux baseline that ships GTK 4 and libadwaita:
# Enterprise Linux 9 (AlmaLinux 9, glibc 2.34, supported until 2032).
#
# Everything is compiled against glibc 2.34, so the results run on any x86_64
# distribution with glibc >= 2.34 (RHEL/Alma/Rocky 9, Ubuntu 22.04, Debian 12,
# Fedora 35+, openSUSE Leap 15.6+... and every newer release). glibc itself is
# never bundled. See docs/design/packaging-pipeline.md.
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

FROM build AS appimage
RUN packaging/linux/appimage.sh /pkgroot /dist

FROM scratch AS dist
COPY --from=packages /dist /
COPY --from=appimage /dist /

# Clean export of just the AppImage (use: --target appimage-out --output type=local,dest=dist)
FROM scratch AS appimage-out
COPY --from=appimage /dist /

