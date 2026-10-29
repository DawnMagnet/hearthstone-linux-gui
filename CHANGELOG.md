# Changelog

## v0.4.0 - 2026-10-29

### Added

- Uninstall asks for confirmation before deleting installed game files.
  Cancel, Escape, and closing the dialog leave the install in place (#61).
- Region and game-language dropdowns show the catalog names. Config files
  and the game still receive the original codes, and the labels refresh
  when the interface language changes (#62).

### Changed

- The support target is every distribution that ships GTK 4. Native deb,
  rpm and pacman packages target GTK 4.6 and libadwaita 1.1 (Ubuntu 22.04,
  Debian 12, Enterprise Linux 9, Fedora 36 and newer). They still compile
  against the GTK 4.4 API so Enterprise Linux 9's Pango 1.48 can link them
  (#57, #64).
- The AppImage is built on AlmaLinux 8 (glibc 2.28), with GTK 4.12 and
  libadwaita 1.4 compiled from source, and runs on Debian 11, Ubuntu 20.04
  and newer. Native packages stay on the AlmaLinux 9 build (#63).
- The interface language switches in the open window. Headers, menus,
  dialogs and status text update in place; the window is not rebuilt, and
  an install, login or running game no longer defers the change until the
  next start (#56).
- BLTE decoding, the content MD5 check and the file write run off the async
  runtime. Decompression uses zlib-rs (flate2 1.1.10): about 1.5× faster
  decompression, and installs about 1.1–1.8× faster (#82).
- Rust dependencies are upgraded within that GTK 4.6 baseline. aes 0.9 and
  cbc 0.2, pbkdf2 0.13 (sha1 and sha2 on digest 0.11), and the rustls stack
  (rustls, rustls-webpki, webpki-roots, quinn-proto) are in. `cargo audit`
  is clean on both lock files. relm4 stays at 0.10.1, reqwest at 0.12, gtk4
  at 0.10 and libadwaita at 0.8. The workspace MSRV is 1.92; the login
  helper stays on 1.88 (#76, #78, #81).

### Fixed

- With no GameMode or MangoHud wrapper, the AppImage no longer drops the
  game's own `LD_LIBRARY_PATH` when it restores the host environment. Mono
  and the plugin directories stay on the path (#58).

### Maintenance

- CI runs `cargo audit` on `Cargo.lock` and `login-helper/Cargo.lock` (#77).
- Dependabot opens at most one grouped version-update pull request per
  ecosystem each month. Security updates stay ungrouped and open
  immediately (#75).
- Pull requests run rustfmt, clippy `-D warnings` and the workspace tests.
  The GTK 4.6 / libadwaita 1.1 feature ceiling is checked in one place, and
  the MSRV matrix reads each manifest's `rust-version` (#59).
- NGDP parser, offline install and download-resume paths have synthetic
  fixture tests (#60).
- BLTE, encoding and archive-index parsers return an error instead of
  panicking when a declared size runs past the buffer (#56).
- The Nix flake and `rust-toolchain.toml` pin Rust 1.92.0, the same
  compiler CI installs (#56).
- GitHub Actions are updated: checkout 7, upload-artifact 7,
  download-artifact 8, docker/setup-buildx-action 4 and
  docker/build-push-action 7 (#65, #66, #69, #70, #72).

## v0.3.2 - 2026-10-26

### Added

- French interface translation (`locales/fr.toml`). A `fr_*` system locale
  selects it automatically, and it can also be chosen from the gear menu
  (#47).

### Changed

- Nix packaging uses Crane. Dependency builds are cached apart from the
  application source, `cargoHash` is no longer maintained by hand, and the
  runtime closure dropped from about 4.1 GB to about 2.2 GB because the Rust
  toolchain no longer leaks into it (#54).

### Fixed

- On NixOS the launcher no longer crashes at startup with
  `GstPlay: playbin3 element not found`. The Nix package includes the
  GStreamer plugins, and `wrapGAppsHook4` sets `GST_PLUGIN_SYSTEM_PATH_1_0`
  (#53).

### Maintenance

- `cargoHash` and the release job that recomputed it are gone. Crane reads
  `Cargo.lock` directly, so a `v*` tag no longer rewrites `flake.nix`
  (#50, #51, #55).
- The HSReplay memory reader uses `as_chunks` instead of `chunks_exact` plus
  `try_into().unwrap()`, which clears the `chunks_exact_to_as_chunks` clippy
  lint (#48).
- `docs/commerce-spec.md` records the observed commerce SDK surface (init,
  balance, catalog) from a clean-room capture (#49).

## v0.3.1 - 2026-10-24

### Added

- Optional login window: when the separate program `hearthstone-linux-gui-login`
  (`login-helper/`, needs WebKitGTK 6) is installed, **Login** shows Blizzard's
  login page in its own dark window and hands the token back directly. It
  always offers **Open in browser**, and the browser flow is used whenever the
  program is missing, fails or the user asks for it. The launcher and its
  packages do not depend on WebKitGTK; a CI job builds the program.
- After the game starts, the launcher can upload the collection to
  HSReplay.net (#44).

### Changed

- **Animated logo.** The window shows a 10-second looping 3D animation of
  Tux at the tavern counter instead of the still logo: the fire flickers,
  Tux breathes, blinks and looks over his cards. It plays through GTK's
  media backend (GStreamer, VP9 with alpha) and pauses while the window is
  minimised or the game runs. Without a media backend (for example in the
  AppImage, whose bundled GTK has none) or when it does not get ready within
  three seconds, the drawn logo shows as before. DEB and RPM packages
  recommend the GStreamer plugins it needs. The Blender scripts that build
  and animate the scene are in `docs/design/logo3d/` and are not part of the
  packages.
- The status line above the logo shows the Unity version on its own line
  below the game version, so the narrow window no longer cuts it short.

### Fixed

- VSync off now also takes effect when the game keeps requesting it: with
  `vsync=0` in `options.txt` the launcher sets `vblank_mode=0`,
  `__GL_SYNC_TO_VBLANK=0` and `MESA_VK_WSI_PRESENT_MODE=immediate` for the
  game. Variables already set in the environment are left untouched, and a
  missing or unreadable `options.txt` changes nothing.
- The game window now shows the launcher icon (#45).
- The download status no longer flickers while Hearthstone installs. The
  speed used to drop out of the line every time a file finished, so the text
  kept changing its length and wrapping point, and the whole column changed
  width with it. The speed is now kept between measurements, the action
  (`Downloading Hearthstone:`) stays on its own line with the counts below
  it and the speed below those, and the digits have equal widths. While an
  install or uninstall runs, the headline keeps room for those three lines,
  so the controls below it move down once when the task starts and back up
  once when it ends instead of jumping with every step. Each status message
  also stays up for at least 0.6 s while a task runs; faster steps are
  skipped, so a quick update check no longer flashes a dozen messages.
  When the task ends, the window shrinks back to its smallest height (it
  only ever grew before); a width the user chose is kept.

### Maintenance

- CI builds less: pull requests only run type-check and unit tests, a push to
  master runs one quick package build, and only `v*` tags build everything
  (AppImage, native packages and Flatpak in parallel). A single `publish` job
  collects the artifacts, writes `SHA256SUMS.txt` and creates the release,
  using `docs/release-notes/<tag>.md` as the release notes when that file
  exists.

## v0.3.0 - 2026-10-15

### Added

- **System information** in the gear menu: launcher version, install kind and
  package manager, distribution, kernel, session, graphics driver module,
  OpenGL renderer and driver/Mesa version, GameMode, MangoHud and the GTK and
  libadwaita versions, with a button that copies them for a bug report.
- While a login is pending, a short hint explains that Blizzard's login page
  is made for the game's built-in browser and its text can be hard to read in
  a regular browser.
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
  locale; a change applies at once, or on the next start while an install,
  a login or the game is running.
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
- Optional update check, **off by default** (switch in the gear menu). When
  it is on and a newer launcher release exists, a glowing upgrade button
  appears below the gear menu and opens that release's page, which says how
  to get it for each package format. Nothing is downloaded or installed by
  the launcher, and the answer is reused for six hours. Packagers whose
  distribution upgrades the launcher itself can build with
  `HEARTHSTONE_LINUX_NO_UPDATE_CHECK=1` to remove the feature.
- Translated desktop entry comment and AppStream summary and description.
- German README (`README.de.md`).

### Changed

- The window content is centred: status, settings and actions form one
  ordered column.
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
