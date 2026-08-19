{
  pkgs,
  pname,
  packageVersion,
  appId,
  desktopFile,
  iconFile,
  hearthstonePackage,
}:

let
  appImageFile = "${pname}-${packageVersion}-x86_64.AppImage";

  x11RuntimeInputs = with pkgs; [
    libice
    libsm
    libx11
    libxscrnsaver
    libxcursor
    libxdamage
    libxext
    libxfixes
    libxi
    libxinerama
    libxrandr
    libxrender
    libxtst
    libxxf86vm
    libxcb
  ];

  graphicsRuntimeInputs =
    with pkgs;
    [
      libdrm
      libglvnd
      libxkbcommon
      wayland
      xkeyboard_config
    ]
    ++ x11RuntimeInputs;

  portableRuntimeInputs =
    with pkgs;
    [
      alsa-lib
      at-spi2-atk
      at-spi2-core
      cairo
      dbus.lib
      dconf.lib
      expat
      fontconfig
      freetype
      gdk-pixbuf
      glib
      glib-networking
      glibc
      graphene
      gsettings-desktop-schemas
      gtk4
      libadwaita
      libepoxy
      libpulseaudio
      pango
      zlib
    ]
    ++ graphicsRuntimeInputs;

  appimageTool = pkgs.appimageTools.extractType2 {
    pname = "appimagetool";
    version = "continuous";
    src = pkgs.fetchurl {
      url = "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage";
      sha256 = "sha256-ptceK2zWb46NFsN60WRliYXgz1/KqVDJCkgokMudE+A=";
    };
  };

  appimageRuntime = pkgs.fetchurl {
    url = "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64";
    sha256 = "sha256-HMSbzx4szVk8N5rbF8n4WjbWGQiCllBN6VsdBiFa678=";
  };

  # The nixpkgs-pinned patchelf (0.15.2 as of writing; patchelf is pinned
  # very conservatively in nixpkgs because of its role in stdenv's own
  # bootstrap) reproducibly hits `*** stack smashing detected ***` on
  # `--set-interpreter` against the Hearthstone Unity player binary, when
  # run at AppImage runtime through the manual `ld.so --library-path`
  # invocation needed to make a dynamically-linked Nix binary portable to
  # non-Nix systems. Use a statically-linked official release instead: it
  # sidesteps the bug (newer patchelf) and the whole class of problem (no
  # dynamic loader indirection needed at all for a static binary).
  patchelfStatic = pkgs.runCommand "patchelf-0.18.0-x86_64" {
    nativeBuildInputs = [ pkgs.gnutar pkgs.gzip ];
  } ''
    mkdir -p $out
    tar -xzf ${
      pkgs.fetchurl {
        url = "https://github.com/NixOS/patchelf/releases/download/0.18.0/patchelf-0.18.0-x86_64.tar.gz";
        sha256 = "sha256-zoTyRH+3qGeeWLxUog3CsBs3tYAuEsV+7OdypvFL8/A=";
      }
    } -C $out
  '';

  appDir =
    pkgs.runCommand "hearthstone-linux-gui-AppDir"
      {
        nativeBuildInputs = with pkgs; [
          binutils
          coreutils
          desktop-file-utils
          findutils
          glib
          librsvg
          patchelf
          pax-utils
        ];
      }
      ''
        mkdir -p \
          $out/usr/bin \
          $out/usr/lib \
          $out/usr/lib/hearthstone-linux-gui-runtime \
          $out/usr/share/applications \
          $out/usr/share/icons/hicolor/128x128/apps \
          $out/usr/share/icons/hicolor/256x256/apps \
          $out/usr/share/icons/hicolor/scalable/apps \
          $out/usr/share/metainfo

        install -Dm755 ${hearthstonePackage}/bin/.hearthstone-linux-gui-wrapped \
          $out/usr/bin/hearthstone-linux-gui
        cp -a ${hearthstonePackage}/share/. $out/usr/share/
        chmod -R u+w $out/usr/share
        rsvg-convert -w 128 -h 128 ${../packaging/appimage/io.github.hearthstone_linux_gui.svg} \
          -o $out/usr/share/icons/hicolor/128x128/apps/${appId}.png
        rsvg-convert -w 256 -h 256 ${../packaging/appimage/io.github.hearthstone_linux_gui.svg} \
          -o $out/usr/share/icons/hicolor/256x256/apps/${appId}.png
        install -Dm755 ${../packaging/appimage/AppRun} $out/AppRun

        ln -s usr/share/applications/${desktopFile} $out/${desktopFile}
        ln -s usr/share/icons/hicolor/scalable/apps/${iconFile} $out/${iconFile}
        ln -s usr/share/icons/hicolor/256x256/apps/${appId}.png $out/${appId}.png
        ln -s ${appId}.png $out/.DirIcon

        copy_lib() {
          local lib="$1"
          local name
          name="$(basename "$lib")"
          if [ -L "$lib" ]; then
            local target target_name
            target="$(readlink -f "$lib")"
            target_name="$(basename "$target")"
            if [ "$name" = "$target_name" ]; then
              install -Dm755 "$target" "$out/usr/lib/$name"
            elif [ ! -e "$out/usr/lib/$target_name" ]; then
              install -Dm755 "$target" "$out/usr/lib/$target_name"
              ln -sfn "$target_name" "$out/usr/lib/$name"
            else
              ln -sfn "$target_name" "$out/usr/lib/$name"
            fi
          else
            install -Dm755 "$lib" "$out/usr/lib/$name"
          fi
        }

        copy_elf_deps() {
          local elf="$1"
          readelf -d "$elf" >/dev/null 2>&1 || return 0
          while IFS= read -r dep; do
            case "$dep" in
              /nix/store/*/lib/*.so*)
                copy_lib "$dep"
                ;;
            esac
          done < <(lddtree -l "$elf")
        }

        copy_elf_deps $out/usr/bin/hearthstone-linux-gui

        for input in ${pkgs.lib.concatStringsSep " " portableRuntimeInputs}; do
          if [ -d "$input/lib/gio/modules" ]; then
            mkdir -p $out/usr/lib/gio/modules
            for module in "$input"/lib/gio/modules/*.so; do
              [ -e "$module" ] || continue
              install -Dm755 "$module" "$out/usr/lib/gio/modules/$(basename "$module")"
            done
          fi
          if [ -d "$input/lib/gdk-pixbuf-2.0" ]; then
            mkdir -p $out/usr/lib/gdk-pixbuf-2.0
            cp -aL "$input"/lib/gdk-pixbuf-2.0/. $out/usr/lib/gdk-pixbuf-2.0/
            chmod -R u+w $out/usr/lib/gdk-pixbuf-2.0
          fi
          if [ -d "$input/share/glib-2.0/schemas" ]; then
            mkdir -p $out/usr/share/glib-2.0/schemas
            cp -L "$input"/share/glib-2.0/schemas/*.xml $out/usr/share/glib-2.0/schemas/ 2>/dev/null || true
          fi
          if [ -d "$input/share/gsettings-schemas" ]; then
            find "$input/share/gsettings-schemas" -path '*/glib-2.0/schemas/*.xml' \
              -exec cp -L {} $out/usr/share/glib-2.0/schemas/ \;
          fi
          if [ -d "$input/share/X11/xkb" ]; then
            mkdir -p $out/usr/share/X11
            cp -aL "$input/share/X11/xkb" $out/usr/share/X11/
            chmod -R u+w $out/usr/share/X11/xkb
          fi
        done

        for module_dir in $out/usr/lib/gio/modules $out/usr/lib/gdk-pixbuf-2.0; do
          [ -d "$module_dir" ] || continue
          while IFS= read -r -d "" module; do
            copy_elf_deps "$module"
          done < <(find "$module_dir" -type f -name '*.so' -print0)
        done

        if [ -d $out/usr/share/glib-2.0/schemas ]; then
          glib-compile-schemas $out/usr/share/glib-2.0/schemas
        fi

        install -Dm755 ${pkgs.glibc.out}/lib/ld-linux-x86-64.so.2 \
          $out/usr/lib/hearthstone-linux-gui-runtime/ld-linux-x86-64.so.2
        install -Dm755 ${patchelfStatic}/bin/patchelf \
          $out/usr/bin/patchelf

        fix_portable_elf() {
          local elf="$1"
          case "$(basename "$elf")" in
            ld-*.so* | ld-linux*.so*)
              return 0
              ;;
          esac
          readelf -d "$elf" >/dev/null 2>&1 || return 0

          while IFS= read -r needed; do
            local basename_needed
            basename_needed="$(basename "$needed")"
            if [ -e "$out/usr/lib/$basename_needed" ]; then
              patchelf --replace-needed "$needed" "$basename_needed" "$elf"
            fi
          done < <(
            readelf -d "$elf" \
              | sed -n 's/.*Shared library: \[\(\/nix\/store\/[^]]*\/lib\/[^]]*\)\].*/\1/p'
          )
        }

        while IFS= read -r -d "" elf; do
          fix_portable_elf "$elf"
        done < <(find $out/usr/bin $out/usr/lib -type f -print0)

        patchelf --set-rpath '$ORIGIN/../lib:$ORIGIN/../lib/hearthstone-linux-gui-runtime' \
          $out/usr/bin/hearthstone-linux-gui
        while IFS= read -r -d "" elf; do
          readelf -d "$elf" >/dev/null 2>&1 || continue
          case "$elf" in
            $out/usr/bin/*)
              ;;
            */ld-*.so* | */ld-linux*.so*)
              ;;
            *)
              patchelf --set-rpath '$ORIGIN:$ORIGIN/hearthstone-linux-gui-runtime' "$elf"
              ;;
          esac
        done < <(find $out/usr/lib -type f -print0)

        find $out/usr/bin $out/usr/lib -type f -executable \
          ! -name 'ld-*.so*' \
          ! -name 'ld-linux*.so*' \
          -exec strip --strip-unneeded {} + 2>/dev/null || true
      '';

  appImage =
    pkgs.runCommand appImageFile
      {
        nativeBuildInputs = with pkgs; [
          coreutils
          desktop-file-utils
          patchelf
          squashfsTools
        ];
      }
      ''
        mkdir -p $out
        cp -a ${appimageTool} appimagetool
        chmod -R u+w appimagetool
        patch_dynamic_tool() {
          local tool="$1"
          local rpath="$2"
          [ -e "$tool" ] || return 0
          readelf -l "$tool" 2>/dev/null | grep -q 'Requesting program interpreter' || return 0
          patchelf --set-interpreter ${pkgs.glibc.out}/lib/ld-linux-x86-64.so.2 \
            --set-rpath "$rpath" \
            "$tool"
        }

        patch_dynamic_tool appimagetool/usr/bin/appimagetool \
          "${
            pkgs.lib.makeLibraryPath [
              pkgs.glibc
              pkgs.zlib
              pkgs.glib
              pkgs.libuuid
            ]
          }:$(pwd)/appimagetool/usr/lib"
        patch_dynamic_tool appimagetool/usr/lib/appimagekit/mksquashfs \
          "${
            pkgs.lib.makeLibraryPath [
              pkgs.glibc
              pkgs.zlib
            ]
          }"
        cp -a ${appDir} AppDir
        chmod -R u+w AppDir
        ARCH=x86_64 ./appimagetool/usr/bin/appimagetool \
          --runtime-file ${appimageRuntime} \
          AppDir $out/${appImageFile}
      '';
in
{
  inherit appDir appImage appImageFile;
}
