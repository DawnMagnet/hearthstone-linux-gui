# AppImage Devcontainer Debugging

Open this repository with VS Code and run `Dev Containers: Reopen in Container`.
The container is Debian 12, uses rustup-provided `rustc`/`cargo`, and has
`linuxdeploy` and `nfpm` preinstalled for manual debugging.

## Quick Checks

```sh
rustc --version
cargo --version
/opt/linuxdeploy/usr/bin/linuxdeploy --version
nfpm --version
```

## Build And Verify GUI Dependencies

```sh
cargo build --workspace --release
ldd target/release/hearthstone-linux-gui | tee /tmp/hearthstone-linux-gui.ldd
! grep -E 'libgtk-[34]\.so|libadwaita-1\.so' /tmp/hearthstone-linux-gui.ldd
cargo test --workspace --release
```

## Inspect FLTK Build Inputs

These checks cover the FLTK bundled build dependencies used by the container.

```sh
pkg-config --libs x11 xext xft xinerama xcursor xrender xfixes
pkg-config --libs pango cairo
cmake --version
```

## Full Script Trial

After the individual checks look sane, run the release script in the container:

```sh
rm -rf /pkgroot /dist /tmp/hearthstone-linux-gui.AppDir /tmp/linuxdeploy-output /tmp/appimage-*
TARGETARCH=amd64 bash -x packaging/native/build-dist.sh
find /dist -maxdepth 1 -type f -printf '%f %s bytes\n' | sort
```

If the final AppImage cannot execute directly inside the container, use the
script's extracted AppRun fallback and dependency scan before testing the
generated `.AppImage` on the host desktop.
