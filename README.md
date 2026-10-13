# Hearthstone Linux Launcher

<p align="center">
  <img src="data/icons/io.github.hearthstone_linux_gui.svg" alt="Tux playing cards: the launcher icon" width="160">
</p>

<p align="center">
  <img src="docs/images/readme-hero.svg" alt="hearthstone-linux-gui desktop launcher preview" width="920">
</p>

<p align="center">
  <a href="README.cn.md">中文说明</a>
  ·
  <a href="README.de.md">Deutsch</a>
  ·
  <a href="https://github.com/DawnMagnet/hearthstone-linux-gui/releases/latest">Latest release</a>
</p>

<p align="center">
  <img alt="Linux x86_64" src="https://img.shields.io/badge/Linux-x86__64-2f6f73?style=flat-square">
  <img alt="GTK4" src="https://img.shields.io/badge/GTK-4-4a86cf?style=flat-square">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-native-b94700?style=flat-square">
  <img alt="Packages" src="https://img.shields.io/badge/AppImage%20%7C%20DEB%20%7C%20RPM%20%7C%20Nix-ready-6750a4?style=flat-square">
</p>

**Hearthstone Linux Launcher** (`hearthstone-linux-gui`) is an unofficial,
community-made native GTK4 desktop manager for installing,
updating, logging into, and launching Hearthstone on Linux. It is migrated from
the original [`hearthstone-linux`](https://github.com/0xf4b1/hearthstone-linux)
project, but the old script-driven workflow has been replaced by a packaged
Rust application with a graphical interface.

No terminal workflow is required for normal users. Download a release, install
or open it with your desktop environment, click **Install / Update**, click
**Login**, then click **Play**.

## What Changed From The Original Project

The original `hearthstone-linux` proved that Hearthstone can run on Linux by
combining Blizzard's official game files with Unity's Linux runtime. This
project keeps that idea, but turns it into a desktop application that is easier
to distribute and maintain.

<p align="center">
  <img src="docs/images/readme-flow.svg" alt="Old script workflow replaced by a GTK4 desktop workflow" width="860">
</p>

| Original `hearthstone-linux` | This project |
| --- | --- |
| Script-oriented setup | GTK4/libadwaita desktop application |
| Manual command-line flow | Button-driven install, login, update, and launch |
| Python/Bash toolchain expected by users | Packaged runtime; no Python or Bash environment needed for normal use |
| External `keg` downloader workflow | Native Rust NGDP downloader with cache and verification |
| Distro-specific setup pain | AppImage, DEB, RPM, and native Nix package outputs |

## Highlights

- **One-click desktop experience**: install, update, login, and launch from a
  GTK4 window.
- **Translated interface**: English, German and Simplified Chinese; the
  launcher follows your system language and can be switched in the gear menu
  at the top right.
- **GameMode and MangoHud**: optional switches start the game through
  `gamemoderun` and with the MangoHud overlay when they are installed.
- **Game settings editor**: change the frame-rate limit, VSync and graphics
  quality, or edit `options.txt` and extra `client.config` entries, from the
  gear menu.
- **No command-line requirement**: release builds are meant for graphical
  installation and daily use.
- **No Python/Bash runtime requirement**: the launcher is a native Rust binary
  packaged with the libraries it needs.
- **Cross-distribution Linux packaging**: use the same project on NixOS,
  Debian, Ubuntu, Fedora, and other x86_64 Linux distributions.
- **Portable AppImage**: GTK/runtime layer bundled; glibc and drivers come from the host (glibc ≥ 2.34).
- **Native Nix package**: Nix users can consume a standard package output with
  desktop integration.
- **DEB/RPM installers**: package-manager friendly builds for common desktop
  distributions.
- **Resumable downloads**: Unity runtime downloads can continue from a partial
  file after interruption.
- **Cached game data**: downloaded NGDP content is cached and verified to avoid
  unnecessary network work.
- **No Steam dependency**: the AppImage carries a portable GTK layer (not
  glibc); the game itself is launched with the project's own runtime handling,
  not `steam-run`.

## Packages

<p align="center">
  <img src="docs/images/readme-packages.svg" alt="Release package formats: AppImage, DEB, RPM, and Nix" width="860">
</p>

| Package | Best for | User experience |
| --- | --- | --- |
| **AppImage** | Any x86_64 Linux desktop with glibc 2.34 or newer | Download and open the application directly |
| **DEB** | Debian, Ubuntu, Linux Mint, Pop!_OS, and related systems | Open with the graphical software installer |
| **RPM** | Fedora, RHEL-compatible, openSUSE-style workflows | Open with the graphical software installer |
| **Flatpak** | Any distribution with Flatpak, including immutable ones (Bazzite, Silverblue, SteamOS) and systems older than glibc 2.34 | `flatpak install --user ./hearthstone-linux-gui-*-x86_64.flatpak` |
| **Nix** | NixOS and Nix package users | Native package output with desktop file and launcher |

### System requirements

All release artifacts are built on the oldest still-supported distribution
that ships GTK 4 and libadwaita, Enterprise Linux 9 (glibc 2.34), so they run
on old and new systems alike:

| | Minimum |
| --- | --- |
| CPU | x86_64 |
| glibc | **2.34** (RHEL/AlmaLinux/Rocky 9, Ubuntu 22.04, Debian 12, Fedora 35, openSUSE Leap 15.6/16 and newer) |
| GTK / libadwaita (DEB/RPM/Pacman) | GTK 4.0, libadwaita 1.0, GLib 2.66 from your distribution |
| GTK / libadwaita (AppImage) | Bundled (GTK 4.12, libadwaita 1.4); glibc, graphics drivers, fonts and X11/Wayland come from your system |
| Flatpak | Only Flatpak itself; the GNOME 51 runtime (with its own glibc and GTK) is installed from Flathub |

On NixOS, use the Nix package below; the AppImage needs `programs.nix-ld`
or `appimage-run`. Older systems such as Ubuntu 20.04, Debian 11 or RHEL 8
are not supported: they do not ship GTK 4.

The AppImage and the DEB/RPM/Pacman packages come from one container build
(`packaging/linux/Dockerfile`); the Nix package is built by the flake. See
[`docs/design/packaging-pipeline.md`](docs/design/packaging-pipeline.md)
and [`packaging/README.md`](packaging/README.md).

### Building packages

All non-Nix release artifacts are built on **AlmaLinux 9** (glibc **2.34**),
the oldest still-supported distribution that ships GTK 4 and libadwaita.
Newer hosts run these binaries without change.

```sh
# deb + rpm + pacman + AppImage → dist/
packaging/appimage/build.sh

# AppImage only
TARGET=appimage-out packaging/appimage/build.sh
```

The AppImage is assembled with pinned **linuxdeploy** /
**appimagetool** / **type2-runtime**. It bundles GTK/libadwaita/GLib from
EL9, but **never** bundles glibc or exports `LD_LIBRARY_PATH` (libraries are
found via `RUNPATH`). Native deb/rpm/pacman packages link against the host
GTK stack. CI (`.github/workflows/packaging.yml`) smoke-tests every artifact
on AlmaLinux 9, Ubuntu 22.04/24.04, Debian 12/13, Fedora, Arch, and openSUSE
Tumbleweed, including the hidden `--print-child-env` probe that catches
environment leaks into child processes.

## Installation For Users

1. Open the
   [latest release page](https://github.com/DawnMagnet/hearthstone-linux-gui/releases/latest).
2. Download the package that matches your system.
3. Open it with your desktop environment:
   - AppImage: open the downloaded AppImage. If your file manager asks for it,
     enable "Allow executing file as program" in file properties.
   - DEB/RPM: double-click the package and install it with your graphical
     software center.
   - Flatpak: `flatpak install --user ./hearthstone-linux-gui-*-x86_64.flatpak`
     (Flathub is added automatically for the GNOME runtime), then
     `flatpak run io.github.hearthstone_linux_gui`. Game files and settings
     live in `~/.var/app/io.github.hearthstone_linux_gui/`.
   - Nix/NixOS: install the native Nix package through your normal Nix workflow.
4. Launch **Hearthstone Linux Launcher** from the application menu.

After the app opens, the normal flow is:

1. Choose your **Region** and **Game language**. On first start both are
   preselected from your system language.
2. Click **Install / Update**.
3. Click **Login** and complete Battle.net login in your browser.
4. Return to the app and click **Play**.

The app stores user data under standard XDG locations in your home directory.
Interrupted Unity downloads are resumed automatically, and already downloaded
game data is reused when possible.

## Nix, NixOS, And Home Manager

This repository is a flake for `x86_64-linux`. It currently exposes package,
app, and development shell outputs; it does not expose dedicated NixOS or Home
Manager modules. Use the package output in your system or home package list.

Run the application directly from GitHub:

```sh
nix run github:DawnMagnet/hearthstone-linux-gui
```

Build outputs from a local checkout:

```sh
nix build .#default
nix build .#runtime
```

The available flake outputs are:

| Output | Purpose |
| --- | --- |
| `packages.x86_64-linux.default` | Native Nix package for the GTK launcher |
| `packages.x86_64-linux.runtime` | FHS runtime wrapper used to launch the downloaded Unity player |
| `apps.x86_64-linux.default` | `nix run` entrypoint |
| `devShells.x86_64-linux.default` | Rust/GTK development shell |

For NixOS flakes, add the repository as an input and install the package with
`environment.systemPackages`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    hearthstone-linux-gui.url = "github:DawnMagnet/hearthstone-linux-gui";
  };

  outputs = { nixpkgs, hearthstone-linux-gui, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [
            hearthstone-linux-gui.packages.${pkgs.stdenv.hostPlatform.system}.default
          ];
        })
      ];
    };
  };
}
```

For standalone Home Manager flakes, add it to `home.packages`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager.url = "github:nix-community/home-manager";
    hearthstone-linux-gui.url = "github:DawnMagnet/hearthstone-linux-gui";
  };

  outputs = { nixpkgs, home-manager, hearthstone-linux-gui, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      homeConfigurations.my-user = home-manager.lib.homeManagerConfiguration {
        inherit pkgs;
        modules = [
          {
            home.username = "my-user";
            home.homeDirectory = "/home/my-user";
            home.stateVersion = "25.05";
            home.packages = [
              hearthstone-linux-gui.packages.${system}.default
            ];
          }
        ];
      };
    };
}
```

If you use Home Manager as a NixOS module, put the same package expression in
`home-manager.users.<name>.home.packages`.

## How It Works

Hearthstone ships official data through Blizzard's NGDP distribution system.
The game is built with Unity, and the Linux Unity player can run the game data
after the platform layout is adapted.

This launcher automates that process:

1. Downloads the official Hearthstone game data for the selected region and
   locale.
2. Verifies and caches downloaded content.
3. Transforms the macOS-style payload into a Linux-ready layout.
4. Detects the required Unity version and installs the matching Linux Unity
   player.
5. Installs compatibility files and configuration needed by the game.
6. Registers the login callback handler and stores the encrypted token locally.
7. Launches the game through a controlled Linux runtime environment.

No proprietary Hearthstone files are stored in this repository or shipped in
the launcher packages. The application retrieves official files from their
upstream distribution endpoints during installation.

## Status And Limitations

- Target architecture: **x86_64 Linux**.
- The game client runs, but this project is unofficial.
- The in-game shop may remain unavailable depending on upstream behavior.
- Use at your own risk. This project is not affiliated with Blizzard
  Entertainment.

## Troubleshooting

| Symptom | What to try |
| --- | --- |
| The app says **Login Required** | Click **Login** again and finish the browser flow (see [Browser login](#browser-login)). |
| `--write-token` says the token format is invalid | Keep the region prefix and both dashes (`XX-<32 characters>-<account id>`); the account id length varies between accounts. |
| The game closes the Battle.net connection with TLS errors | See [Certificate failures on rolling distros](#certificate-failures-on-rolling-distros). |
| The game window is solid white / flickers at launch | See [White screen on some AMD GPUs](#white-screen-on-some-amd-gpus). |
| Install was interrupted | Click **Install / Update** again; resumable and cached downloads will be reused. |
| The game does not launch after an update | Click **Install / Update** once to repair Unity/runtime files. |
| The game exits immediately with `Unable to load mono library` (NixOS flake) | Update the flake; the FHS environment now ships `zlib`, which the bundled Mono runtime requires. |
| A package opens but does not start on an unusual distro | Try the AppImage release (bundles GTK; uses your system glibc). |

### Browser login

Clicking **Login** registers the launcher as the system handler for the
`blizzard-hearthstone://` URL scheme and opens the Battle.net sign-in page.
After you sign in, the browser redirects to that scheme and the launcher
writes the login token automatically — no copy-paste needed. The token is
stored encrypted at `<game dir>/token`.

If the browser instead shows `http://localhost:0/...` failing with
`ERR_UNSAFE_PORT` (Chrome) or `NS_ERROR_PORT_ACCESS_NOT_ALLOWED`
(Firefox), the handler was not registered or your release predates the
callback fix. You can finish the login manually:

1. Copy the value of the `ST=` parameter from the blocked URL (everything
   after `ST=` up to the next `&`).
2. Run:
   ```sh
   hearthstone-linux-gui --write-token 'US-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx-1234567'
   ```
   With the AppImage: `./hearthstone-linux-gui_*.AppImage --write-token '...'`.
   Passing the whole failed URL to `--auth-callback` also works.

A valid token is a two-character region prefix, a dash, 32 alphanumeric
characters, a dash, and your numeric account id. If the command still rejects
the value, check that the region prefix was not stripped while copying.

### Certificate failures on rolling distros

If the game starts but cannot sign in to Battle.net, and
`~/.config/unity3d/Blizzard Entertainment/Hearthstone/Player.log` or
`<game dir>/Logs/**/BattleNet.log` contains
`Curl error 35 ... UnityTls error code: 7` or
`Ssl error ... CERTIFICATE_VERIFY_FAILED` / `ERROR_SDK_SOCKET_CLOSED (1901)`,
the game cannot find the system certificate store. The Unity player hard-codes
Debian/RHEL CA bundle paths that do not exist on openSUSE and some other
rolling distros. Related upstream discussion:
[0xf4b1/hearthstone-linux#108](https://github.com/0xf4b1/hearthstone-linux/issues/108).

On openSUSE Tumbleweed:

```sh
# 1) Provide the CA bundle paths the Unity player expects (once, needs root)
sudo ln -sf /var/lib/ca-certificates/ca-bundle.pem /etc/ssl/certs/ca-bundle.crt
sudo ln -sf /var/lib/ca-certificates/ca-bundle.pem /etc/ssl/certs/ca-certificates.crt

# 2) Import the system roots into Mono's user trust store
#    (cert-sync comes with the distro mono-core package)
cert-sync --user /etc/ssl/certs/ca-bundle.crt
```

If `BattleNet.log` still reports a missing `libmono-btls-shared.so`, copy that
library from your distro's `mono-core` package into
`<game dir>/Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/` and launch with
`MONO_TLS_PROVIDER=btls`.

### White screen on some AMD GPUs

On some AMD integrated GPUs (for example Rembrandt / Radeon 680M), the game
window renders solid white and flickers, while `Player.log` shows
`ERROR: Shader Hidden/Universal Render Pipeline/DBufferClear shader is not
supported on this GPU`. The Linux-repackaged game data is missing the shader
variants these feature levels need; this cannot be fixed from the launcher at
runtime and is tracked in [#13](https://github.com/DawnMagnet/hearthstone-linux-gui/issues/13).
Forcing a different renderer (`-force-glcore`, `-force-vulkan`) does not help,
but enabling **Use discrete GPU** in the launcher may.

Release builds default to INFO-level logging. Detailed diagnostic logs can be
enabled by developers with the standard `RUST_LOG` environment variable when
debugging locally.

## Translations

Interface texts live in `locales/<language>.toml` and are compiled into the
binary; English (`en.toml`) is the base and the fallback. To add a language,
copy `en.toml`, translate the values, and register the file in
`src/i18n.rs`. `cargo test` reports missing, unknown or unused keys and
placeholders that differ from the English catalog.

## Legal Notice

Hearthstone, Battle.net, Blizzard Entertainment, and related names, trademarks,
game assets, services, and other materials are owned by Blizzard Entertainment,
Inc. or its affiliates and licensors.

This project is an unofficial compatibility launcher. It is not produced,
published, sponsored, approved, endorsed, maintained, or supported by Blizzard
Entertainment, Battle.net, Microsoft, Activision Blizzard, or any of their
affiliates. No proprietary Hearthstone game assets are stored in this
repository or shipped in the launcher packages.

See [LEGAL.md](LEGAL.md) for the full legal information, project boundaries,
user responsibilities, and rights-holder request process.
