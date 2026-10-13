# Changelog

## Unreleased

### Added

- New display name **Hearthstone Linux Launcher** with the subtitle
  "Unofficial community launcher" in the window title, the desktop entry and
  the AppStream metadata. Binary name, package name and application ID are
  unchanged.
- New icon and logo: Tux playing cards at a table in a tavern, an original drawing
  without Blizzard material. The logo is shown below the window title.
- The launcher is translatable. All interface texts, progress messages and
  command-line help come from locale catalogs in `locales/`, which are
  compiled into the binary. English is the base and the fallback; German and
  Simplified Chinese are included.
- New launcher language selection in a gear menu at the top right,
  with a flag per language (shipped as graphics, independent of emoji fonts). By default the launcher follows the system
  locale; after a change the launcher restarts itself to apply it.
- On first start, region and game language are preselected from the system
  locale instead of always starting with `us` / `enUS`.
- **GameMode** and **MangoHud** switches. The game is started as
  `gamemoderun mangohud <game>`; a switch is disabled with an explanation
  when the tool is not installed or, in the Flatpak, not reachable from the
  sandbox.
- **Game settings** dialog in the gear menu: frame-rate limit, VSync and
  graphics quality, plus plain editors for the game's `options.txt` and for
  additional `client.config` entries. VSync is switched off by writing
  `vsync=0`. Unknown keys and their order are preserved, and the dialog is
  locked while the game runs because the game rewrites the file on exit.
- Additional `client.config` entries are stored outside the game directory
  and re-applied on every Install / Update instead of being overwritten.
- Optional update check at startup (switch in the gear menu). When a newer
  launcher release exists, a glowing upgrade button appears below the gear
  menu. It downloads the DEB or RPM for this installation and opens
  it with the desktop's software installer; for AppImage, Flatpak, Nix and
  Pacman it opens the release page.
- Translated desktop entry comment and AppStream summary and description.
- German README (`README.de.md`).

### Changed

- The window content is centred: status, settings and actions form one
  ordered column, and the status line stays on one line.
- Login and Play share one button: it shows **Login** while logged out and
  **Play** once logged in; **Log Out** and **Switch Account** are in its
  menu.
- **Refresh** is an icon button next to the status line.
- The window opens at its smallest size.
- The status line sits above the logo on two lines; it and the settings
  start flush with the first action button.
- The **Locale** setting is now labelled **Game language** to tell it apart
  from the launcher language.

### Fixed

- The Play button no longer derives its "stopping" state from the status
  text.

### Maintenance

- The icon moved from `packaging/appimage/` to `data/icons/` as the single
  source for all package formats.
- `cargo test` fails when a catalog misses a key, has an unknown or unused
  key, or uses different placeholders than the English catalog.

## v0.2.3 - 2026-10-11

### Fixed

- AppImage: host programs started by the launcher (browser openers, `gio`,
  `xdg-mime`, `update-desktop-database`, `patchelf`, the game) no longer
  inherit the AppImage's bundled glibc/GLib. `AppRun` records the original
  environment as `HEARTHSTONE_LINUX_HOST_*` and no longer exports the bundled
  library directory in `LD_LIBRARY_PATH`; the launcher gets it through
  `ld-linux --library-path` only. This made the Login button silently do
  nothing and the game exit after about a second on Fedora/Bazzite and other
  hosts whose glibc differs from the bundled one (#22, #23).
- The game only gets the bundled libraries on its library path when it runs
  through the bundled ELF interpreter.
- Login opens the browser through the XDG Desktop Portal when available, waits
  for each opener's exit status and falls through to the next one on failure,
  and runs off the UI thread.
- The full login URL is always shown with a Copy button while a login is
  pending, so it can be pasted manually when no browser appears.
- When the game exits within 15 seconds or fails, the status line shows the
  exit code or signal and points to `Player.log` and the new game output log
  (`<state dir>/logs/game.log`).

### Changed

- Rebuilt AppImage and native packages on AlmaLinux 9 (glibc 2.34) with
  linuxdeploy. The AppImage bundles GTK 4 / libadwaita / GLib only and never
  ships glibc; `AppRun` no longer sets `LD_LIBRARY_PATH`. Native deb/rpm/pacman
  packages declare the same glibc 2.34 floor and RPM uses soname dependencies
  (#24).

### Maintenance

- Added `--print-child-env` and multi-distro Packaging CI smoke checks.
- Fixed Packaging CI smoke SIGPIPE, AppImage ETXTBSY on overlayfs, headless
  GSK renderer selection, and skipped AppImage GUI smoke on rolling openSUSE
  where the EL9-bundled GUI can segfault against host X11/Mesa.

## v0.2.2 - 2026-10-02

### Fixed

- Registered the `blizzard-hearthstone://` login callback for all regions, so
  the browser hands the login token back to the launcher automatically instead
  of redirecting to `localhost:0`, which browsers block (#12, #14).
- Accepted login tokens with any account id length instead of hard-coding 45
  characters, which rejected valid tokens from accounts with shorter ids
  (#10).
- Made ELF interpreter patching best-effort: a broken or missing `patchelf`
  no longer aborts the launch, the system `patchelf` is used as a fallback
  when the bundled one crashes, and the game starts with its existing
  interpreter otherwise (#9).
- Added `zlib` to the Nix FHS runtime so the bundled Mono runtime can load on
  NixOS (#8).

### Maintenance

- Expanded the troubleshooting documentation in both READMEs, covering the
  browser login flow and its manual fallback, TLS certificate setup on
  rolling distributions, and the known white-screen shader limitation on
  some AMD GPUs (#9, #13).
- Refreshed the AppImage runtime hash after the upstream continuous artifact
  was updated.

## v0.2.1 - 2026-08-19

### Fixed

- Fixed the Unity black-screen startup failure caused by macOS's absolute
  CoreFoundation import path not resolving on Linux.
- Reworked the compatibility fix to patch managed assemblies to use the
  standard Linux library name, avoiding a dependency on NixOS, FHS wrappers,
  or bubblewrap mount namespaces.
- Kept patched managed assemblies user-writable so future launches and updates
  remain repeatable.

### Maintenance

- Added regression coverage for rewriting the CoreFoundation import.

## v0.2.0 - 2026-06-19

### Added

- Added a managed uninstall action for the installed game files.

### Changed

- Reworked install and account actions to use libadwaita split buttons, keeping
  common actions on the main button and less frequent actions in the dropdown.

### Maintenance

- Refreshed the Nix Cargo vendor hash for the 0.2.0 workspace version.

## v0.1.9 - 2026-06-17

### Fixed

- Switched Unity release lookup to the current Release API after the previous
  GraphQL endpoint began returning 404 responses.
- Updated the footer copyright year and displayed the application version.

### Maintenance

- Centralized the package version in the Cargo workspace metadata and reused it
  from Nix packaging.

## v0.1.8 - 2026-06-16

### Changed

- Simplified the install architecture by extracting shared filesystem,
  formatting, and cancellation helpers into `util`.
- Split NGDP installation internals into focused modules for install planning,
  local install manifests, and parallel install execution.
- Reworked compatibility stub installation into a table-driven flow while
  preserving the installed file layout.
- Unified install, download, Unity, NGDP, and UI polling cancellation around
  `tokio_util::sync::CancellationToken`.
- Derived region and locale string parsing/display behavior with `strum` and
  added tests to lock the existing string formats.

### Fixed

- Preserved replacement of read-only compatibility stubs while keeping copied
  files user-writable.
- Kept local NGDP manifest validation behavior intact after module extraction.
- Kept cancellation reporting consistent between UI-triggered installation
  stops and worker failures.

### Maintenance

- Removed unused direct dependencies: `async-channel`, `thiserror`, and `xdg`.
- Added direct dependencies on `tokio-util` and `strum`.
- Added focused tests for region and locale string parsing/display.
