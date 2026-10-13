# Roadmap

[Deutsche Fassung](ROADMAP.de.md)

Starting point is v0.2.3: a GTK4/libadwaita launcher (Rust, relm4) that
installs the macOS game data through NGDP, pairs it with the Linux Unity
player, handles the Battle.net login and starts the game. This roadmap takes it
to 1.0 in four releases. Each release is usable on its own. Releases 0.3.0 to
0.5.0 are developed on one branch and proposed upstream as a single pull
request once 0.5.0 is complete; the shop follows separately.

| Release | Theme | Size |
| --- | --- | --- |
| 0.3.0 | Translatable UI (locale catalog) | medium |
| 0.4.0 | Launch options (GameMode, MangoHud) and game settings editor | medium |
| 0.5.0 | New name, window title, logo and icon | small code, art work |
| 1.0.0 | In-game shop through the original macOS commerce library | large, research first |

The catalog comes first on purpose: 0.4.0 and 0.5.0 add many new strings, and
they should be written against the catalog from the start instead of being
converted afterwards.

## Where the code stands today

| Area | State | Location |
| --- | --- | --- |
| Launch | Starts `Bin/Hearthstone.x86_64` directly or through the Nix runtime runner; only launch option is "Discrete GPU" | `src/install/launcher.rs` (`launch_game`, `graphics_env`) |
| Launcher settings | `config.toml` with region, locale, `use_discrete_gpu` | `src/config.rs`, `SettingsPanel` in `src/ui/components.rs` |
| Game configuration | `client.config` is regenerated from a template on every install; the game's own `options.txt` is never touched | `src/install/compatibility.rs`, `assets/client.config.in` |
| UI strings | About 70 hard-coded English strings; no catalog | `src/ui/app.rs`, `components.rs`, `status.rs`, `src/main.rs` |
| Name and title | Window title, desktop entry and AppStream name are all `hearthstone-linux-gui` | `src/ui/app.rs:116`, `data/` |
| Icon | One SVG, kept under the AppImage packaging | `packaging/appimage/io.github.hearthstone_linux_gui.svg` |
| Shop | A stub answers 88 of the 338 entry points the game binds and returns an empty shop; the HTTP client stub is a no-op | `crates/stubs/blz-commerce-sdk-plugin`, `crates/stubs/commerce-http-client` |

## 0.3.0 Translatable UI

Goal: no user-visible string is hard-coded; English is base and fallback;
German ships complete.

- Add a locale catalog that is compiled into the binary (`locales/en.*`,
  `locales/de.*`). Embedded catalogs avoid installing `.mo` files in five
  package formats and keep the glibc 2.34 build free of a `libintl`
  dependency.
- Move every string from `src/ui/` and the CLI messages in `src/main.rs`
  behind catalog keys, including formatted ones (`"Stop {action}"`,
  `"Failed: {message}"`).
- `launch_state()` in `src/ui/app.rs` currently derives state by comparing the
  headline text (`"Stopping game"`). Replace that with a real state field
  before translating, otherwise the logic breaks in every other language.
- UI language follows the system locale. It can be overridden in a gear menu
  at the top right, which lists each language with its flag (shipped
  as graphics, not emoji, so it does not depend on installed fonts). It is
  independent of the game language that already exists.
- On first start, region and game language are preselected from the system
  locale instead of always starting with `us` / `enUS`.
- The window content is centred so the status, the settings and the actions
  read as one ordered column.
- Translate the desktop entry (`Name[de]`, `Comment[de]`) and the AppStream
  metainfo (`xml:lang`).
- A test fails the build when a key is missing in a non-English catalog or is
  unused.
- Chinese is kept in sync as a third catalog; add `README.de.md` and a German
  screenshot next to the existing Chinese README.

Status: implemented on the working branch.

Done when: the app runs fully in German with `LANG=de_DE.UTF-8`, falls back to
English for an unsupported locale, and the catalog test is part of CI.

## 0.4.0 Launch options and game settings

### GameMode and MangoHud

- Two switches in the settings panel, stored in `config.toml`
  (`use_gamemode`, `use_mangohud`), default off.
- `launch_game` builds a wrapper chain in front of the executable:
  `gamemoderun mangohud <game>`. The wrappers have to be resolved on the host
  `PATH` (`host_env`), not inside the AppImage environment.
- A switch is disabled with an explanatory tooltip when the tool is not
  installed. Detection runs at start and on "Refresh".
- Packaging differences:
  - AppImage, DEB, RPM, Pacman: host tools are used directly.
  - Flatpak: the host tools are not reachable from the sandbox, so the
    switches are disabled and say so. Making them work needs the
    `org.freedesktop.Platform.VulkanLayer.MangoHud` extension and access to
    the GameMode D-Bus service; that is left for a follow-up.
  - Nix: the wrappers are put in front of the runtime runner; whether the
    FHS environment needs the MangoHud and GameMode libraries added has to
    be tested on NixOS.
- The wrapper chain is covered by a unit test.

### Game settings editor

The game reads two files, and they need different handling:

| File | Location | Content | Problem today |
| --- | --- | --- | --- |
| `options.txt` | `~/Library/Preferences/Blizzard/Hearthstone/` (the macOS path, used as is on Linux) | Graphics and gameplay options, `key=value` | Not exposed at all |
| `client.config` | game directory | Connection and locale settings | Overwritten on every Install / Update |

- A "Game settings" dialog, opened from the gear menu, with typed controls
  for the common cases: frame-rate limit (`targetframerate`), VSync on or
  off, graphics quality.
- An "Advanced" section with a plain text editor for `options.txt` and for
  additional `client.config` entries, with "Open in editor" and "Reset to
  defaults".
- Unknown keys and their order are preserved on save. The game rewrites
  `options.txt` on exit, so editing is blocked while the game runs.
- User changes to `client.config` are kept as an overlay that is re-applied
  after the template is rendered, so they survive Install / Update.
- VSync is switched off by adding `vsync=0` to `options.txt`. The key is not
  present by default, so the editor adds it when needed and removes it (or
  sets `vsync=1`) to switch VSync back on.

Done when: frame rate and VSync can be changed from the launcher, the change
is visible in the game (verified with the MangoHud overlay), and survives an
update.

Status: implemented on the working branch. Writing the files is verified;
the effect inside the running game still has to be confirmed.

## 0.5.0 Name, logo and icon

### Name and window title

The technical name `hearthstone-linux-gui` stays as binary name, package name
and application ID (`io.github.hearthstone_linux_gui`), so existing installs,
data directories and the registered login handler keep working. Only the
displayed name changes.

- Proposed display name: **Hearthstone Linux Launcher**, with the subtitle
  "Unofficial community launcher". Window title, desktop entry, AppStream
  name and the About text all use it through the catalog.
- The subtitle is not optional. `LEGAL.md` commits the project to never
  looking like an official Blizzard product, and a launcher that looks more
  professional needs the disclaimer more, not less.

### Logo and icon

- Motif: Tux playing cards at a table inside a tavern (fireplace, window,
  lanterns). Tux is the only character; glossy, rounded 3D
  mascot style consistent with the author's other apps, slightly varied.
- No Blizzard material: no Hearthstone logo or swirl, no card frames, card
  art, fonts or screenshots. The cards are an original design.
- Deliverables:
  - App icon as SVG in `data/icons/`, the single source for AppImage, DEB,
    RPM, Pacman, Flatpak and Nix; the packaging scripts render the PNG sizes
    from it.
  - Logo in the main window directly below the title.
  - Icon at the top of the READMEs.
- Reference material used while designing stays outside the repository.
- A symbolic (monochrome) icon is not needed: GNOME does not show one for
  applications that have no notifications or status entry.
- The current drawing is a flat vector version. A rendered 3D version in the
  glossy mascot style can replace it later without further code changes.

Done when: all package formats show the new icon and name in the application
menu, and the logo renders correctly in light and dark mode.

Status: implemented on the working branch; the packaged icon still has to be
checked in each package format.

## 1.0.0 In-game shop

### Idea

The installed game is the macOS build, and its shop is implemented by a
native macOS library that the launcher currently skips and replaces with an
empty stub. Instead of reimplementing the shop, find out how it works on
macOS and then build a **minimal emulation layer** that lets exactly this
part load and run on Linux:

- Blizzard's original commerce library is loaded unmodified.
- Around it, a small layer provides only the piece of macOS environment this
  library actually uses.
- Its inputs and outputs are mapped to a compatibility layer the Linux Unity
  player can work with, replacing today's stub.

The launcher then contains no commerce logic of its own: catalog, prices,
checkout and payment are handled by Blizzard's code talking to Blizzard's
servers. The original files are never patched.

### What is known so far

- The game binds 338 entry points of `blz_commerce_sdk_plugin` (SWIG
  bindings in `Blizzard.Commerce.Bindings.dll`); today's stub answers 88 of
  them. The interface is handle based: handles and strings can be passed
  through without knowing the structures behind them.
- Events come back through a listener callback, driven by
  `blz_commerce_update` once per frame.
- Checkout is not a separate window. The library drives an off-screen
  browser (`BlizzardBrowser.app`, Chromium based, already downloaded with
  the game) and hands frames to the game, which draws them and sends mouse
  and keyboard input back (`blz_commerce_register_scene`,
  `blz_commerce_browser_send_event`).
- The macOS library itself is a Mach-O bundle. Linux cannot load it directly;
  it needs a loader and the system functions it imports.

### Phase 1: understand the macOS shop

Nothing is built before this is answered, because the answers decide how
small the emulation layer can be.

1. Keep the original commerce library at install time (in its own directory,
   verified like all other NGDP content) instead of skipping it.
2. Read its Mach-O load commands: which system libraries and frameworks it
   links, and the full list of imported symbols. This list **is** the size
   of the macOS environment to simulate.
3. Sort the imports: plain C and C++ runtime (maps almost one to one to
   Linux), CoreFoundation (the project already ships a partial stub),
   networking and TLS, and anything Objective-C or Cocoa.
4. Trace the flow on the managed side: initialisation, HTTP registration,
   product load, personalized shop, checkout, purchase events.
5. Find out how the library talks to `BlizzardBrowser`: how it is started,
   which channel carries frames and input.

Result: a written import and flow map, and a go or no-go for each of the two
parts below.

### Phase 2: emulation layer for the commerce library

```
Hearthstone (Linux Unity player)
  │  P/Invoke, 338 entry points
  ▼
libblz_commerce_sdk_plugin.so      ← ours: compatibility layer
  │  loads and relocates the Mach-O image, binds its imports
  ▼
original macOS commerce library    ← Blizzard's, unmodified
  │  imported system calls
  ▼
macOS environment shim             ← ours: only what Phase 1 found in use
  │
  ▼
Linux libc, TLS, sockets, files
```

- A Mach-O loader inside the compatibility library: map the segments, apply
  relocations and bind the import table to the shim. Both sides are x86_64
  and use the same C calling convention, so no CPU emulation is involved.
- The shim implements the imported functions on top of Linux equivalents.
  Paths the library expects under `~/Library` are mapped to the game's own
  directories.
- The exported entry points are generated from the bindings assembly, so a
  game update that adds entry points is a rebuild, not manual work.
- If the original library is missing or fails to load, the layer behaves
  like today's stub: the shop stays empty and the game runs normally.

Milestone: the shop shows real products.

### Phase 3: checkout browser

`BlizzardBrowser.app` is a complete Chromium application for macOS. It
depends on Cocoa and the macOS graphics stack, which a minimal layer cannot
simulate. The realistic route is to keep the protocol and replace the
process: a small Linux off-screen browser that speaks the channel found in
Phase 1 and loads the same Blizzard checkout pages. Whether this is
acceptable, or whether checkout should open in the system browser instead,
is decided with the Phase 1 results.

Milestone: a purchase made by the account owner arrives in the game.

### Risks

- The import list may turn out large, or include Objective-C runtime use.
  Each such dependency is either shimmed, or it ends this approach.
- C++ runtime differences (macOS `libc++` versus Linux `libstdc++`) have to
  be handled by shipping a matching runtime for the loaded image.
- Fallback if Phase 1 says no: run the **Windows** build of the same library
  unmodified in a helper process under Wine and bridge it to the game. It
  keeps the same principle (original code, untouched) at the cost of an
  optional Wine dependency.
- A native reimplementation of the shop protocol stays ruled out.

### Rules for this release

- Session tokens and anything that passes the layer stay on the machine and
  are never written to logs.
- The launcher never initiates, alters or repeats a purchase. It forwards
  calls, nothing else.
- Development is done with catalog browsing only. Real purchases as a final
  test are made by the account owner, deliberately, with a small amount.
- `LEGAL.md` is rewritten for this release. Today it says the project does
  not touch commerce, and the stub section says the stubs give no access
  beyond the official client. Both statements need to describe the new
  design accurately: original library, unmodified, fetched from Blizzard by
  the user's own install, not redistributed.
- "More license-conformant" is a technical statement about not
  reimplementing and not modifying Blizzard code. It is not a legal
  clearance: running the client on Linux is outside what Blizzard supports,
  and that stays true with a working shop. The README says so plainly.

Done when: the shop lists products and a purchase made by the account owner
arrives in the game, the game still runs when the layer cannot load, and an
update of the game does not need manual changes.

## Decisions

1. **Repository.** Decided: the work is contributed to the upstream project.
   Releases 0.3.0 to 0.5.0 go in one pull request once 0.5.0 is complete.
   Project URL, copyright line and application ID stay as they are.
2. **Catalog format.** Decided: catalogs embedded in the binary, no new
   dependency.
3. **Shop.** Decided: emulate the macOS environment for the original macOS
   library, with the Wine route only as fallback.
4. **Display name.** Decided: "Hearthstone Linux Launcher" with the subtitle
   "Unofficial community launcher".
