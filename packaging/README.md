# Packaging

The Debian 12 Dockerfile is the release packaging path. It uses apt for system
dependencies, installs the Rust toolchain with rustup, builds the Rust FLTK
application once, creates native package formats with nfpm, then builds the
AppImage from the same executable with linuxdeploy.

This project has four release tracks:

- `deb`: native `.deb` package for Debian/Ubuntu-style systems.
- `rpm`: native `.rpm` package for Fedora/RHEL-style systems.
- `pacman`: native `.pkg.tar.zst` package for pacman-based systems.
- `appimage`: self-contained x86_64 AppImage, built after the native packages.

## All Artifacts

Build everything with one command:

```sh
docker build \
  --target dist \
  --file packaging/native/Dockerfile \
  --output type=local,dest=dist/release \
  .
```

The output contains:

- `*.deb`: Debian package.
- `*.rpm`: RPM package.
- `*.pkg.tar.zst`: pacman package.
- `*.AppImage`: portable x86_64 AppImage.
- `SHA256SUMS.txt`: checksums.

## AppImage

Build and copy only the AppImage to `dist/`:

```sh
packaging/appimage/build.sh
```

## Deb/RPM/pacman

Build and copy native package formats to `dist/packages/`:

```sh
packaging/deb-rpm/build.sh
```
