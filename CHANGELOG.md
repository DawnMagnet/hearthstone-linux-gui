# Changelog

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
