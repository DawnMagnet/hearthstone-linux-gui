# Packaging

This project has three release tracks:

- `nix`: a native Nix package from `flake.nix`.
- `appimage`: a self-contained x86_64 AppImage for general Linux use.
- `deb-rpm`: `.deb` and `.rpm` packages that install the AppImage payload.

The `.deb` and `.rpm` packages intentionally reuse the AppImage instead of
linking the GTK/libadwaita launcher against each target distro. That keeps the
package-format installers broad while the AppImage carries the portable runtime.

## Nix

Build the native package:

```sh
nix build .#default
```

Build only the Nix runtime wrapper used to launch the downloaded Unity player:

```sh
nix build .#runtime
```

## AppImage

Build on an old-enough x86_64 Linux baseline, for example Ubuntu 22.04 or newer.
The build host should have `cargo`, `linuxdeploy`, `appimagetool`, `patchelf`,
and standard GTK development packages installed.

```sh
packaging/appimage/build.sh
```

The output is written to `dist/`.

## Deb/RPM

Build the AppImage first, then wrap it with nfpm:

```sh
packaging/deb-rpm/build.sh dist/Hearthstone_Linux-x86_64.AppImage
```

This produces both `.deb` and `.rpm` files in `dist/packages/`.
