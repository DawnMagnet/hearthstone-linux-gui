# Roadmap

Starting point is v0.2.3: a GTK4/libadwaita launcher (Rust, relm4) that
installs the macOS game data through NGDP, pairs it with the Linux Unity
player, handles the Battle.net login and starts the game. Each step below is
usable on its own and lands as a small, focused pull request. The per-step
details live in the pull requests and in `CHANGELOG.md`.

| Step | Theme | State |
| --- | --- | --- |
| 0.3.0 | Translatable UI (locale catalogs in `locales/`, German and Chinese) | merged (#29) |
| 0.4.0 | Launch options (GameMode, MangoHud) and the game settings editor | merged (#30, #31) |
| 0.5.0 | Display name, logo and icon (compact layout and update check follow) | in review |
| 1.0.0 | In-game shop | open question, see below |

## 0.5.0 Name, logo and icon

- The displayed name is **Hearthstone Linux Launcher**. The binary name,
  package name and application ID (`io.github.hearthstone_linux_gui`) stay as
  they are, so existing installs, data directories and the registered login
  handler keep working.
- Everything that shows the name also says that this is an unofficial
  community launcher: the window title has it as its subtitle, and the
  desktop entry and AppStream summary start with it. `LEGAL.md` commits the
  project to never looking like an official Blizzard product, and a launcher
  that looks more professional needs the disclaimer more, not less.
- The icon is an original drawing (Tux playing cards at a table in a tavern)
  without any Blizzard material: no logo, card frames, card art or fonts. It
  lives in `data/icons/`, the single source for all package formats, and is
  shown as a logo below the window title.
- Open: check the icon in the Flatpak, AppImage and Nix application menus.

## 1.0.0 In-game shop

The installed game is the macOS build, whose shop is implemented by a native
macOS library that the launcher skips and replaces with an empty stub
(`crates/stubs/blz-commerce-sdk-plugin`, `crates/stubs/commerce-http-client`).

Whether and how the shop can be supported is an open question that depends on
a legal path first. It is discussed in #26; nothing is planned or committed
here until that discussion has a result.
