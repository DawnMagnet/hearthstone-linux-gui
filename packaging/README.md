# Packaging

Nix is the source of truth for release builds.

A rebuild of this pipeline that keeps bundled glibc/GLib out of host
processes and builds the AppImage from the same Debian 12 baseline as the
native packages is planned in
[`docs/design/packaging-pipeline.md`](../docs/design/packaging-pipeline.md).

This project exposes two build tracks:

- `nix`: the native Nix package from `flake.nix`.
- `appimage`: a portable x86_64 AppImage defined in `nix/appimage.nix`.

## Nix

Build the native package:

```sh
nix build .#default
```

Build the Nix runtime wrapper used to launch the downloaded Unity player:

```sh
nix build .#runtime
```

## AppImage

Build the AppImage:

```sh
nix build .#AppImage
```

`packaging/appimage/build.sh` is a convenience wrapper that copies the AppImage
to `dist/`.

### AppImage environment rules

`AppRun` records the host value of every variable it overrides as
`HEARTHSTONE_LINUX_HOST_<NAME>` (or `HEARTHSTONE_LINUX_HOST_UNSET_<NAME>`),
and passes the bundled library directory to the launcher only through
`ld-linux --library-path`. It never exports it in `LD_LIBRARY_PATH`. The
launcher (`src/host_env.rs`) restores the host environment for every program
it starts. When adding a variable to `AppRun`, also add it to the recording
loop and to `host_env::OVERRIDDEN_VARS`. `tests/apprun.rs` covers this
contract.
