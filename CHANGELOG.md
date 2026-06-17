# Changelog

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
