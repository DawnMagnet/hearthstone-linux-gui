# Packaging

Release artifacts come from two build tracks:

- **Container build** (`packaging/linux/`): the AppImage and the deb, rpm and
  pacman packages, all built on **AlmaLinux 9 (glibc 2.34)**, the oldest
  still-supported distribution that ships GTK 4 and libadwaita. Every artifact
  runs on any x86_64 distribution with glibc >= 2.34, old and new.
- **Nix** (`flake.nix`): the native Nix package for Nix/NixOS users.

Why this baseline and what was rejected is in
[`docs/design/packaging-pipeline.md`](../docs/design/packaging-pipeline.md).

## Minimum requirements

| Artifact | glibc | GTK / libadwaita |
| --- | --- | --- |
| AppImage | 2.34 | bundled (GTK 4.12, libadwaita 1.4, GLib 2.68 from EL9) |
| deb / rpm / pacman | 2.34 | host: GTK >= 4.0, libadwaita >= 1.0, GLib >= 2.66, Pango >= 1.46 |

The GTK floor comes from the gtk-rs feature levels: `Cargo.toml` disables
relm4's default `gnome_42` feature. Enabling newer feature levels raises the
minimum GTK/GLib; check the table above and the CI smoke matrix when doing so.

## Container build

```sh
packaging/appimage/build.sh            # docker or podman, output in dist/
# or directly:
docker build -f packaging/linux/Dockerfile --target dist --output type=local,dest=dist .
# AppImage only:
TARGET=appimage-out packaging/appimage/build.sh
```

If the AlmaLinux mirror list is slow, pass
`--build-arg ALMALINUX_MIRROR=https://ftp.osuosl.org/pub/almalinux`.

| File | Purpose |
| --- | --- |
| `linux/Dockerfile` | AlmaLinux 9 toolchain; stages `build`, `packages`, `appimage`, `dist` |
| `linux/stage.sh` | Installs the release build into an FHS tree shared by all formats |
| `linux/packages.sh` | deb/rpm/pacman via nfpm (soname RPM dependencies) |
| `linux/appimage.sh` | AppImage via pinned linuxdeploy, appimagetool and type2 runtime |
| `linux/check-glibc.sh` | Fails if any ELF needs glibc > 2.34 or core runtime libraries got bundled |
| `linux/smoke.sh` | Installs and starts the artifacts on a distro (CI matrix) |
| `appimage/AppRun` | AppImage entry point |

Pinned tools (bump the version and the SHA-256 together in `appimage.sh`):
linuxdeploy `1-alpha-20251107-1`, appimagetool `1.9.1`, type2 runtime
`20251108`.

`.github/workflows/packaging.yml` builds everything on every pull request
and smoke-tests it on AlmaLinux 9, Ubuntu 22.04, Debian 12, Ubuntu 24.04,
Debian 13, Fedora, Arch Linux and openSUSE Tumbleweed.

### AppImage rules

- glibc, `ld-linux`, libstdc++, libgcc_s, GL/EGL, libdrm, X11/xcb core,
  libwayland-client, fontconfig, freetype and HarfBuzz always come from the
  host (linuxdeploy excludelist; enforced by `check-glibc.sh`).
- `LD_LIBRARY_PATH` is never set; bundled libraries are found via `RUNPATH`.
- `AppRun` records the host value of every variable it sets as
  `HEARTHSTONE_LINUX_HOST_<NAME>` (or `HEARTHSTONE_LINUX_HOST_UNSET_<NAME>`).
  The launcher (`src/host_env.rs`) restores the host environment for every
  program it starts. When adding a variable to `AppRun`, also add it to the
  recording loop and to `host_env::OVERRIDDEN_VARS`; `tests/apprun.rs` fails
  otherwise.
- Bundled GTK only loads bundled modules (gdk-pixbuf loaders, dconf, ibus).

## Nix

```sh
nix build .#default   # native Nix package
nix build .#runtime   # FHS runtime wrapper used to launch the Unity player
```
