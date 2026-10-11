# Packaging

Release artifacts come from two build tracks:

- **Container build** (`packaging/linux/`): the AppImage and the deb, rpm and
  pacman packages, all built on **AlmaLinux 9 (glibc 2.34)**, the oldest
  still-supported distribution that ships GTK 4 and libadwaita. Every artifact
  runs on any x86_64 distribution with glibc ≥ 2.34, old and new.
- **Flatpak** (`packaging/flatpak/`): a single-file bundle on the GNOME 51
  runtime from Flathub, built by `.github/workflows/flatpak.yml`.
- **Nix** (`flake.nix`): the native Nix package for Nix/NixOS users.

Why this baseline and what was rejected is in
[`docs/design/packaging-pipeline.md`](../docs/design/packaging-pipeline.md).

## Minimum requirements

| Artifact | glibc | GTK / libadwaita |
| --- | --- | --- |
| AppImage | 2.34 | bundled (GTK 4.12, libadwaita 1.4, GLib 2.68 from EL9) |
| deb / rpm / pacman | 2.34 | host: GTK ≥ 4.0, libadwaita ≥ 1.0, GLib ≥ 2.66, Pango ≥ 1.46 |

The GTK floor comes from the gtk-rs feature levels: `Cargo.toml` disables
relm4's default `gnome_42` feature. Enabling newer feature levels raises the
minimum GTK/GLib; check the table above and the CI smoke matrix when doing so.

## How to build

Requires Docker or Podman with BuildKit (`docker buildx` / `podman build`
with `--output`).

```sh
# All artifacts (deb, rpm, pacman, AppImage) → dist/
packaging/appimage/build.sh

# Same thing, explicit docker invocation:
docker build -f packaging/linux/Dockerfile --target dist --output type=local,dest=dist .

# AppImage only (faster iteration):
TARGET=appimage-out packaging/appimage/build.sh
```

If the AlmaLinux mirror list is slow, pass
`--build-arg ALMALINUX_MIRROR=https://ftp.osuosl.org/pub/almalinux`
(or set it in `packaging/appimage/build.sh` via the environment).

| File | Purpose |
| --- | --- |
| `linux/Dockerfile` | AlmaLinux 9 toolchain; stages `build`, `packages`, `appimage`, `dist` |
| `linux/stage.sh` | Installs the release build into an FHS tree shared by all formats |
| `linux/packages.sh` | deb/rpm/pacman via nfpm (soname RPM dependencies) |
| `linux/appimage.sh` | AppImage via pinned linuxdeploy, appimagetool and type2 runtime |
| `linux/check-glibc.sh` | Fails if any ELF needs glibc > 2.34 or core runtime libraries got bundled |
| `linux/smoke.sh` | Installs and starts the artifacts on a distro (CI matrix); runs `--print-child-env` |
| `appimage/AppRun` | AppImage entry point |
| `appimage/build.sh` | Convenience wrapper around the Dockerfile |

## Pinned tools

Bump the version and the SHA-256 together in `linux/appimage.sh`:

| Tool | Version |
| --- | --- |
| linuxdeploy | `1-alpha-20251107-1` |
| appimagetool | `1.9.1` |
| type2-runtime | `20251108` |

Do not use the `continuous` releases; they break whenever upstream
republishes them.

## AppImage rules

- glibc, `ld-linux`, libstdc++, libgcc_s, GL/EGL, libdrm, X11/xcb core,
  libwayland-client, fontconfig, freetype and HarfBuzz always come from the
  host (linuxdeploy excludelist; enforced by `check-glibc.sh`).
- `LD_LIBRARY_PATH` is **never** set; bundled libraries are found via
  `RUNPATH`.
- `AppRun` records the host value of every variable it sets as
  `HEARTHSTONE_LINUX_HOST_<NAME>` (or `HEARTHSTONE_LINUX_HOST_UNSET_<NAME>`).
  The launcher (`src/host_env.rs`) restores the host environment for every
  program it starts. When adding a variable to `AppRun`, also add it to the
  recording loop and to `host_env::OVERRIDDEN_VARS`; `tests/apprun.rs` fails
  otherwise.
- Bundled GTK only loads bundled modules (gdk-pixbuf loaders, dconf, ibus).

## Native packages (deb / rpm / pacman)

Produced by `linux/packages.sh` (nfpm) from the same AlmaLinux 9 binary:

- **deb**: versioned Depends (`libc6 (>= 2.34)`, `libgtk-4-1`, …).
- **rpm**: soname Provides/Requires (`libgtk-4.so.1()(64bit)`,
  `libc.so.6(GLIBC_2.34)(64bit)`, …) so Fedora, EL9 and openSUSE resolve them.
- **pacman**: Arch-compatible package from the same staged tree.
- Homepage / AppStream URL: `https://github.com/DawnMagnet/hearthstone-linux-gui`.
- Architecture: **x86_64 only** (Unity player).

## CI smoke matrix

`.github/workflows/packaging.yml` builds everything on every pull request /
`master` push and smoke-tests it on:

| Image | Role |
| --- | --- |
| `almalinux:9` | Oldest baseline (glibc 2.34) |
| `ubuntu:22.04` | Oldest Ubuntu LTS (glibc 2.35) |
| `debian:12` | Oldest Debian with GTK 4 (glibc 2.36) |
| `ubuntu:24.04` | Current Ubuntu LTS |
| `debian:13` | Current Debian |
| `fedora:latest` | Current Fedora |
| `archlinux:latest` | Rolling |
| `opensuse/tumbleweed:latest` | Rolling |

Each smoke job (`packaging/linux/smoke.sh`) installs the matching native
package and the extracted AppImage, checks that libraries resolve (bundled
GTK from the AppImage, glibc from the host), runs `--no-gui`, and keeps the
GUI up for 8 s under Xvfb.

### `--print-child-env`

Hidden diagnostic flag used by smoke and by developers debugging #22-style
environment leaks:

```sh
# From a normal install:
hearthstone-linux-gui --print-child-env

# From an AppImage (also with a poisoned LD_LIBRARY_PATH, as CI does):
LD_LIBRARY_PATH=/tmp/does-not-exist \
  ./hearthstone-linux-gui_*.AppImage --print-child-env
```

It prints every `host_env::OVERRIDDEN_VARS` entry as a freshly spawned host
child actually sees it after restore. A clean run has no AppDir paths and no
bundled `LD_LIBRARY_PATH`. `smoke.sh` fails the job if any leak appears.

## Flatpak

```sh
flatpak-builder --user --install --force-clean build-flatpak \
  packaging/flatpak/io.github.hearthstone_linux_gui.yml
flatpak run io.github.hearthstone_linux_gui
```

The manifest builds with the `rust-stable` SDK extension and lets cargo fetch
the crates pinned in `Cargo.lock` (`--share=network` at build time), so it is
meant for the GitHub bundle, not Flathub. The sandbox gets network, IPC,
Wayland/X11 fallback, PulseAudio and `/dev/dri`; game files and settings live
under `~/.var/app/io.github.hearthstone_linux_gui/`. Login URLs open through
the OpenURI portal, and the exported desktop file claims the Battle.net
callback schemes on the host.

The `Flatpak` workflow runs for tag releases (called from `release.yml`) and
can be started by hand with `release_tag` set to attach the bundle to an
existing release without changing the version:

```sh
gh workflow run flatpak.yml -f release_tag=v0.2.3
```

## Nix

```sh
nix build .#default   # native Nix package
nix build .#runtime   # FHS runtime wrapper used to launch the Unity player
```
