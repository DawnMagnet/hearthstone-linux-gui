FROM ustclug/debian:12

ARG RUST_VERSION=1.90.0
ARG NFPM_VERSION=2.43.1
ARG LINUXDEPLOY_URL=https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
ARG TARGETARCH=amd64

ENV CARGO_HOME=/opt/cargo \
  RUSTUP_HOME=/opt/rustup \
  PATH=/opt/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  APPIMAGE_EXTRACT_AND_RUN=1 \
  CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
  DEBIAN_FRONTEND=noninteractive

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
  appstream \
  bash-completion \
  binutils \
  ca-certificates \
  libcairo2-dev \
  cmake \
  curl \
  dbus-x11 \
  desktop-file-utils \
  dpkg-dev \
  file \
  fuse3 \
  gcc \
  g++ \
  git \
  less \
  libc6-dev \
  libfontconfig1-dev \
  libfreetype-dev \
  libpango1.0-dev \
  librsvg2-bin \
  libx11-dev \
  libxcursor-dev \
  libxext-dev \
  libxfixes-dev \
  libxft-dev \
  libxinerama-dev \
  libxrender-dev \
  make \
  ninja-build \
  patchelf \
  pax-utils \
  pkg-config \
  procps \
  rpm \
  shared-mime-info \
  squashfs-tools \
  vim-tiny \
  xkb-data \
  xz-utils \
  zstd \
  && rm -rf /var/lib/apt/lists/*

RUN case "$TARGETARCH" in \
  amd64) rustup_arch=x86_64-unknown-linux-gnu; nfpm_arch=x86_64 ;; \
  arm64) rustup_arch=aarch64-unknown-linux-gnu; nfpm_arch=arm64 ;; \
  *) echo "unsupported TARGETARCH=$TARGETARCH" >&2; exit 1 ;; \
  esac \
  && curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION" --default-host "$rustup_arch" \
  && rustup component add clippy rustfmt rust-src \
  && rustc --version \
  && cargo --version \
  && curl -fsSL "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_${nfpm_arch}.tar.gz" \
  | tar -xz -C /usr/local/bin nfpm \
  && chmod 755 /usr/local/bin/nfpm \
  && if [ "$TARGETARCH" = "amd64" ]; then \
  curl -4 -fsSL "$LINUXDEPLOY_URL" -o /tmp/linuxdeploy-x86_64.AppImage; \
  linuxdeploy_offset="$(grep -abo hsqs /tmp/linuxdeploy-x86_64.AppImage | tail -n 1 | cut -d: -f1)"; \
  test -n "$linuxdeploy_offset"; \
  unsquashfs -q -o "$linuxdeploy_offset" -d /opt/linuxdeploy /tmp/linuxdeploy-x86_64.AppImage; \
  chmod 755 /opt/linuxdeploy/usr/bin/linuxdeploy /opt/linuxdeploy/usr/bin/linuxdeploy-plugin-appimage; \
  /opt/linuxdeploy/usr/bin/linuxdeploy --version; \
  fi

WORKDIR /workspace
