# Packaging pipeline: audit and rebuild plan

Status: steps 0–3 and 5–6 implemented (step 0 with the fix for #22; steps
1–3 and 5–6 in the follow-up "oldest baseline" change). Step 4 (Flatpak) is
open.

## Why

Issue #22 (Login button does nothing, game exits after ~1 s on Bazzite 44)
was caused by the AppImage, not by the launcher logic: `AppRun` exported the
AppImage's library directory, which contained a Nix-built **glibc 2.42**, in
`LD_LIBRARY_PATH`. Every host program the launcher started (`xdg-open`, `gio`,
the browser, `patchelf`'s wrapper shell, the Unity player) then loaded that
glibc with the host's own `ld-linux`, which fails or crashes whenever the host
glibc differs. Earlier reports (#2, #9, #14) were most likely the same thing.

The general goal for every release artifact is: **use the host's libraries
where possible, and never let bundled core libraries (glibc, GLib, GTK) leak
into processes we start.**

## Current pipeline (after the rebuild)

```
                     ┌─────────────────────────────────────┐
  AlmaLinux 9 build ─┤ cargo build --release                │  glibc 2.34 floor
  container          │ (packaging/linux/Dockerfile)         │
                     └──────────────────┬──────────────────┘
                                        │ packaging/linux/stage.sh
          ┌─────────────────────────────┼──────────────────────────┐
          ▼                             ▼                          ▼
   nfpm: deb / rpm / pacman      AppImage: linuxdeploy +      (later) Flatpak
   host GTK/adw/GLib             appimagetool, bundles
   (packages.sh)                 GTK/adw/GLib but NOT glibc,
                                 ld-linux, libstdc++, GL, X11
                                 (appimage.sh)
   Nix package: unchanged, built by flake.nix for Nix users
```

| Artifact | Built by | Libraries | Assessment |
| --- | --- | --- | --- |
| Nix package (`nix build`) | `flake.nix`, `buildRustPackage` + `wrapGAppsHook4` | Nix store; game runs in a `buildFHSEnv` runtime | Fine on Nix/NixOS. Keep. |
| AppImage | `packaging/linux/appimage.sh` (AlmaLinux 9 + linuxdeploy) | Bundles EL9 GTK 4.12 / libadwaita 1.4 / GLib 2.68 only; **no** glibc, `ld-linux`, libstdc++, GL, X11/Wayland core; no exported `LD_LIBRARY_PATH` | Done. |
| deb / rpm / pacman | `packaging/linux/packages.sh` (AlmaLinux 9 build, nfpm) | Host libraries only (glibc ≥ 2.34, GTK ≥ 4.0, libadwaita ≥ 1.0, GLib ≥ 2.66, Pango ≥ 1.46); RPM soname deps; x86_64 only | Done. |

### Principles

- **One baseline build** for every non-Nix artifact.
- **Never bundle glibc, `ld-linux`, libstdc++, libgcc_s or GL/Vulkan/Wayland
  client/X11 core libraries.** linuxdeploy applies the AppImage excludelist;
  `check-glibc.sh` fails the build if any of these slips in or if any bundled
  file needs a glibc newer than 2.34. The AppImage loads host drivers natively.
- **No exported `LD_LIBRARY_PATH`.** Bundled libraries are found through
  `RUNPATH`. `AppRun` only sets GTK data/module variables (`XDG_DATA_DIRS`,
  `GTK_EXE_PREFIX`, `GTK_PATH`, `GIO_MODULE_DIR`, `GDK_PIXBUF_MODULE_FILE`,
  `GSETTINGS_SCHEMA_DIR`), records their host values, and the launcher
  restores them for children (`host_env.rs`); `tests/apprun.rs` checks that
  every variable pointing into the AppDir is restorable.
- **Bundled GTK only loads bundled modules.** gdk-pixbuf loaders (SVG), the
  dconf GIO module and the ibus input-method module are shipped; host modules,
  built against a different GTK/GLib, are never loaded into the bundled ones.
  The gdk-pixbuf loader cache needs absolute paths, so `AppRun` writes it
  from a template into `$XDG_RUNTIME_DIR` at start-up.
- **The game always uses the host loader.** The AppImage no longer ships
  `ld-linux` or `patchelf`, so there is no interpreter rewriting. NixOS users
  use the Nix package (or `programs.nix-ld`).
- **Pinned tooling**, verified by SHA-256 in `packaging/linux/appimage.sh`:
  linuxdeploy `1-alpha-20251107-1`, appimagetool `1.9.1`, type2 runtime
  `20251108`. The linuxdeploy GTK plugin is not used: it forces
  `GDK_BACKEND=x11` and `GTK_THEME`, and its AppRun hook would not record host
  values.

### Choosing the baseline: oldest practical glibc

The rule is: build on the **oldest distribution that is still supported and
still ships GTK 4 and libadwaita**, so that the binary's glibc symbol versions
are as old as possible, while everything else still runs on the newest
releases (glibc is backwards compatible; newer hosts run older binaries).

| Candidate | glibc | GTK 4 / libadwaita in repos | Supported in 2026 | Verdict |
| --- | --- | --- | --- | --- |
| RHEL/CentOS 7 | 2.17 | no | no | no GTK 4 |
| RHEL 8 / Alma 8 | 2.28 | no | yes (2029) | would need GTK 4, GLib, Pango, ... built from source |
| Debian 11 | 2.31 | no | no (LTS ended 2026-08) | no GTK 4 |
| Ubuntu 20.04 | 2.31 | no | ESM only | no GTK 4 |
| **RHEL 9 / AlmaLinux 9** | **2.34** | **GTK 4.12, libadwaita 1.4 (CRB)** | **yes (2032)** | **chosen** |
| Ubuntu 22.04 | 2.35 | GTK 4.6, libadwaita 1.1 | yes (2027) | fallback |
| Debian 12 | 2.36 | GTK 4.8, libadwaita 1.2 | yes | previous native baseline |

AlmaLinux 9 is the oldest glibc that has GTK 4 and libadwaita as distribution
packages. Going lower (RHEL 8, glibc 2.28) would mean compiling the whole
GTK 4 stack (GLib ≥ 2.66, Pango, cairo, HarfBuzz, graphene, libepoxy,
Wayland, GTK, libadwaita) from source for the AppImage only, while no
distribution with glibc < 2.34 could use the native packages anyway (none of
them ship GTK 4). That is not worth the maintenance cost; Flatpak is the
better answer for such systems.

**Resulting minimums** (enforced by `packaging/linux/check-glibc.sh` in the
build, and by the CI smoke matrix):

- glibc **2.34** for every artifact (binary, stub libraries, every library in
  the AppImage).
- Native packages: GTK **4.0**, libadwaita **1.0**, GLib **2.66**, Pango
  **1.46**. These are the gtk-rs feature levels selected in `Cargo.toml`:
  relm4's default `gnome_42` feature is disabled, so the binary links no
  symbol newer than that, even though it is compiled against EL9's newer
  GTK 4.12. (With `gnome_42` the binary needed GLib 2.72 and Pango 1.50,
  which EL9 does not have.)
- AppImage: bundles GTK 4.12.3, libadwaita 1.4.2, GLib 2.68.4 from EL9.

## Historical audit (pre-rebuild Nix AppImage)

The problems below motivated the rebuild. They no longer apply to the
release AppImage.

1. **Exported bundled glibc** (`LD_LIBRARY_PATH`): the cause of #22. Fixed by
   step 0 (host-env isolation) and then removed entirely by step 1 (no bundled
   glibc, no `LD_LIBRARY_PATH`).
2. **Bundled glibc inside the launcher process itself.** Host Mesa/NVIDIA
   drivers, IM modules and GIO modules could fail to load against a foreign
   glibc. Fixed by not bundling glibc.
3. **The glibc version moved with `flake.lock`** (nixos-unstable). Replaced by
   the fixed AlmaLinux 9 baseline.
4. **ELF interpreter rewriting of the game** via bundled `ld-linux` /
   `patchelf`. Removed; NixOS users use the Nix package.
5. **Unpinned `continuous` appimagetool / type2 runtime.** Replaced by pinned
   SHA-256 releases.
6. **No cross-distro smoke test in CI.** Added in step 2
   (`.github/workflows/packaging.yml`).

Native package issues that were fixed in step 3: RPM soname provides, homepage
pointing at this repository, x86_64-only builds.

## Steps

0. **Host-environment isolation (done with #22).**
   - `AppRun` records the host environment for every variable it overrides.
   - `host_env.rs` restores the host environment for every spawned program.
   - Browser opening prefers the XDG Desktop Portal, checks exit status, and
     always shows the login URL.
   - Quick game exits report their status and log paths.
   - `tests/apprun.rs` checks that `LD_LIBRARY_PATH` is not exported.
1. **AppImage from the baseline build** (done; AlmaLinux 9 / glibc 2.34).
   - `packaging/linux/Dockerfile` (stages `build`, `packages`, `appimage`,
     `dist`) runs `appimage.sh`, which uses linuxdeploy without the GTK
     plugin and adds the GTK runtime pieces itself.
   - Minimal `AppRun`: no `LD_LIBRARY_PATH`, no bundled interpreter.
   - `release.yml` uses it; the Nix-built AppImage (`nix/appimage.nix`,
     `.#AppImage`, `.#AppDir`) is removed rather than kept as `-nix`, because
     it is the artifact that broke #22.
2. **CI smoke matrix** (done, `.github/workflows/packaging.yml`). Oldest
   baselines `almalinux:9`, `ubuntu:22.04`, `debian:12` and current
   `ubuntu:24.04`, `debian:13`, `fedora:latest`, `archlinux`,
   `opensuse/tumbleweed`: `packaging/linux/smoke.sh` installs the native
   package and the extracted AppImage, checks that every library resolves
   (bundled GTK from the AppImage, glibc from the host), runs `--no-gui` and
   keeps the GUI up for 8 s under Xvfb. It also runs the hidden
   `--print-child-env` probe (and a poisoned `LD_LIBRARY_PATH` case) so host
   children do not inherit AppRun's GTK/GLib module paths or a bundled
   `LD_LIBRARY_PATH`. `tests/apprun.rs` still checks AppRun itself.
3. **Native package fixes** (done). Soname-based RPM dependencies (work on
   Fedora, EL9 and openSUSE), versioned deb dependencies at the real API
   floor, fixed homepage, x86_64 only.
4. **Optional: Flatpak** on `org.gnome.Platform`. It is the most robust
   portable format for a GTK app and opens URLs through the portal by design.
   The Unity player would run inside the sandbox on runtime libraries.
   Evaluate after step 2.
5. **Pinned tooling** (done). linuxdeploy / appimagetool / type2 runtime are
   fixed release tags with SHA-256 checks in `appimage.sh`.
6. **Docs** (done). `packaging/README.md` and both READMEs document the glibc
   floor, AppImage rules, pinned tools, CI matrix, `--print-child-env`, and
   how to build.

## How to build and verify

See [`packaging/README.md`](../../packaging/README.md) for the full command
list. Short version:

```sh
packaging/appimage/build.sh                          # all artifacts → dist/
TARGET=appimage-out packaging/appimage/build.sh      # AppImage only
hearthstone-linux-gui --print-child-env              # dump restored child env
```

## Risks

- Hosts with glibc older than 2.34 (Ubuntu 20.04, Debian 11, RHEL 8) cannot
  run any artifact. Today they only worked by accident of the bundled Nix
  glibc, and none of them ships GTK 4. If needed later: build the GTK 4 stack
  from source on AlmaLinux 8 for the AppImage only, or offer a Flatpak.
- The AppImage's GTK is 4.12 (EL9). It gets security fixes through EL9
  updates, but not new GTK features; the native packages use the host GTK.
- Input methods on X11 work through the bundled ibus module (fcitx5 via its
  ibus frontend). On Wayland GTK's built-in text-input protocol is used.
- EL9's mirror network can be slow from some regions; set the
  `ALMALINUX_MIRROR` build argument (for example
  `https://ftp.osuosl.org/pub/almalinux`).
