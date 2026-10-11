# Packaging pipeline: audit and rebuild plan

Status: proposal. The environment isolation part (step 0) ships with the
fix for #22; the remaining steps are planned follow-up work.

## Why

Issue #22 (Login button does nothing, game exits after ~1 s on Bazzite 44)
was caused by the AppImage, not by the launcher logic: `AppRun` exported the
AppImage's library directory, which contains a Nix-built **glibc 2.42**, in
`LD_LIBRARY_PATH`. Every host program the launcher started (`xdg-open`, `gio`,
the browser, `patchelf`'s wrapper shell, the Unity player) then loaded that
glibc with the host's own `ld-linux`, which fails or crashes whenever the host
glibc differs. Earlier reports (#2, #9, #14) were most likely the same thing.

The general goal for every release artifact is: **use the host's libraries
where possible, and never let bundled core libraries (glibc, GLib, GTK) leak
into processes we start.**

## Audit of the current pipeline

| Artifact | Built by | Libraries | Assessment |
| --- | --- | --- | --- |
| Nix package (`nix build`) | `flake.nix`, `buildRustPackage` + `wrapGAppsHook4` | Nix store; game runs in a `buildFHSEnv` runtime | Fine on Nix/NixOS. Keep. |
| AppImage | `nix/appimage.nix` (nixos-unstable closure) | Bundles glibc 2.42, `ld-linux`, GLib, GTK4, libadwaita, X11/Wayland libs, glib-networking, dconf, patchelf | Main source of OS incompatibility; see below. |
| deb / rpm / pacman | `packaging/native/Dockerfile` (Debian 12, nfpm) | Host libraries only (glibc >= 2.36, GTK >= 4.8, libadwaita >= 1.2) | Right model. Small fixes needed. |

### AppImage problems

1. **Exported bundled glibc** (`LD_LIBRARY_PATH`): the cause of #22. Fixed by
   step 0 below.
2. **Bundled glibc inside the launcher process itself.** The launcher still
   runs on Nix's glibc. Host code that gets loaded into it, such as Mesa/NVIDIA
   GL drivers through libglvnd, GTK input-method modules, NSS modules and
   GVfs GIO modules, is built against the host glibc. On a host with a
   *newer* glibc (Fedora 44, Arch, Tumbleweed) those modules can fail to load
   with `GLIBC_2.xx not found`. GTK then falls back to software rendering, or
   in the worst case crashes. This can't be fixed while glibc is bundled.
3. **The glibc version moves with `flake.lock`.** It comes from
   nixos-unstable, so there is no stable "minimum host" and every lock bump
   can change compatibility.
4. **ELF interpreter rewriting of the game.** When the host has no
   `/lib64/ld-linux-x86-64.so.2` (NixOS), the Unity player is patched to use
   the AppImage's `ld-linux`, which lives under a temporary
   `/tmp/.mount_*` path that disappears when the AppImage exits. This is
   fragile and only useful on NixOS, where the Nix package is the better choice
   anyway.
5. **Unpinned tools.** `appimagetool` and the type2 runtime are downloaded
   from the `continuous` releases with fixed hashes, so they break whenever
   upstream republishes them. This already happened for v0.2.2.
6. **No cross-distro smoke test in CI.** The AppImage is only ever executed
   by users.

### Native package problems (minor)

- RPM dependencies are Fedora package names (`gtk4`, `libadwaita`,
  `glib2`, `dbus-libs`). openSUSE uses different names (`libgtk-4-1`,
  `libadwaita-1-0`, ...), so zypper cannot resolve them. Use soname
  dependencies such as `libgtk-4.so.1()(64bit)`, which work on both.
- `homepage` points to the old `DawnMagnet/hearthstone-linux` URL.
- The Dockerfile accepts `TARGETARCH=arm64`, but Hearthstone's Unity player
  is x86_64 only.

## Target pipeline

```
                    ┌─────────────────────────────┐
  Debian 12 build ──┤ cargo build --release        │  (one binary, glibc 2.36 floor)
  container         └──────────────┬──────────────┘
                                   │
          ┌────────────────────────┼─────────────────────────┐
          ▼                        ▼                         ▼
   nfpm: deb / rpm / pacman   AppImage (linuxdeploy +   (later) Flatpak
   host libs only             gtk plugin), bundles      org.gnome.Platform
                              GTK/adw/GLib but NOT
                              glibc / ld-linux / GL
                                   │
   Nix package: unchanged, built by flake.nix for Nix users
```

Principles:

- **One baseline build** for every non-Nix artifact: Debian 12
  (glibc 2.36, GTK 4.8, libadwaita 1.2), the same container that already
  builds deb/rpm/pacman.
- **Never bundle glibc, `ld-linux`, libstdc++ or GL/Vulkan drivers.** Follow
  the AppImage project's exclude list. The AppImage then runs on any distro
  with glibc >= 2.36 and loads host drivers natively.
- **No exported `LD_LIBRARY_PATH`.** Bundled libraries are found through
  `RUNPATH=$ORIGIN/../lib`. `AppRun` keeps recording `HEARTHSTONE_LINUX_HOST_*`
  and the launcher keeps restoring it for children (`host_env.rs`), so the
  GTK variables it does set (`GSETTINGS_SCHEMA_DIR`, `GDK_PIXBUF_MODULE_FILE`,
  `GIO_MODULE_DIR`, ...) never reach the browser or the game.
- **The game always uses the host loader** outside Nix. Drop the interpreter
  rewriting from the AppImage. NixOS users are pointed to the Nix package or
  `programs.nix-ld`.
- **Pinned tooling**: tagged `linuxdeploy`, `linuxdeploy-plugin-gtk` and
  `appimagetool` releases with checksums, not `continuous`.

## Steps

0. **Host-environment isolation (done with #22).**
   - `AppRun` records the host environment and passes the bundled library path
     only through `ld-linux --library-path`.
   - `host_env.rs` restores the host environment for every spawned program.
   - The game gets bundled libraries only when it uses the bundled loader.
   - Browser opening prefers the XDG Desktop Portal, checks exit status, and
     always shows the login URL.
   - Quick game exits report their status and log paths.
   - `tests/apprun.rs` checks that `LD_LIBRARY_PATH` is not exported.
1. **AppImage from the Debian 12 build** (next PR).
   - Add an `appimage` stage to `packaging/native/Dockerfile` that runs
     `linuxdeploy --plugin gtk` on the release binary.
   - Write a minimal `AppRun` with no `LD_LIBRARY_PATH` and no bundled
     interpreter.
   - Switch `release.yml` to it. Keep `nix build .#AppImage` for one release as
     `-nix` for comparison, then remove `nix/appimage.nix`.
2. **CI smoke matrix** (same PR as 1). Run each artifact with `--no-gui` in
   `debian:12`, `ubuntu:24.04`, `fedora:latest`, `archlinux` and
   `opensuse/tumbleweed`: install the deb/rpm/pacman package, or run the
   AppImage with `--appimage-extract-and-run`. Add a hidden
   `--print-child-env` flag and assert that the environment a spawned child
   would get has no AppImage paths in `LD_LIBRARY_PATH`, `GIO_MODULE_DIR`,
   etc.
3. **Native package fixes.** Use soname-based RPM dependencies, fix the
   homepage, and limit native packages to x86_64.
4. **Optional: Flatpak** on `org.gnome.Platform`. It is the most robust
   portable format for a GTK app and opens URLs through the portal by design.
   The Unity player would run inside the sandbox on runtime libraries.
   Evaluate after step 2.
5. **Docs.** Update `packaging/README.md` and the READMEs' install section with
   the glibc floor and which artifact to pick (distro package > AppImage > Nix
   on NixOS).

## Risks

- Hosts with glibc older than 2.36 (Ubuntu 22.04, Debian 11) can no longer run
  the AppImage after step 1. Today they only work by accident of the bundled
  glibc. Mitigation: build on an older baseline with GTK/libadwaita bundled
  from source, or point those users to Flatpak.
- `linuxdeploy-plugin-gtk` sets GTK variables in its own `AppRun` hook. These
  must be added to the `HEARTHSTONE_LINUX_HOST_*` recording list
  (`host_env::OVERRIDDEN_VARS`) or the hook must be replaced.
