{
  description = "Native Rust FLTK hearthstone-linux-gui manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        pname = cargoToml.package.name;
        packageVersion = cargoToml.package.version;
        appId = "io.github.hearthstone_linux_gui";
        desktopFile = "${appId}.desktop";
        iconFile = "${appId}.svg";
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rustfmt"
            "clippy"
          ];
        };
        rustSource = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter =
            path: type:
            let
              root = toString ./.;
              relative = pkgs.lib.removePrefix "${root}/" (toString path);
              top = builtins.head (pkgs.lib.splitString "/" relative);
            in
            builtins.elem top [
              "Cargo.lock"
              "Cargo.toml"
              "assets"
              "crates"
              "data"
              "src"
            ];
        };
        x11Inputs = with pkgs; [
          libice
          libsm
          libx11
          libxcb
          libxcursor
          libxdamage
          libxext
          libxfixes
          libxft
          libxi
          libxinerama
          libxrandr
          libxrender
          libxscrnsaver
          libxtst
          libxxf86vm
          xorgproto
        ];
        fltkInputs =
          with pkgs;
          [
            cairo
            expat
            fontconfig
            freetype
            glib
            libglvnd
            pango
            sysprof
          ]
          ++ x11Inputs;
        fhsRuntimeInputs =
          with pkgs;
          [
            alsa-lib
            cairo
            cups
            dbus
            expat
            fontconfig
            freetype
            gcc.cc.lib
            glib
            libdrm
            libglvnd
            libpulseaudio
            libuuid
            libxkbcommon
            libxml2
            mesa
            nspr
            nss
            openssl
            pango
            vulkan-loader
            wayland
            xkeyboard_config
          ]
          ++ x11Inputs;
        hearthstoneRuntime = pkgs.buildFHSEnv {
          name = "hearthstone-linux-gui-runtime";
          targetPkgs = _: fhsRuntimeInputs;
          runScript = "${pkgs.coreutils}/bin/env";
        };
        nativeBuildInputs = with pkgs; [
          cmake
          desktop-file-utils
          git
          makeWrapper
          pkg-config
          rustToolchain
        ];
        hearthstonePackage = pkgs.rustPlatform.buildRustPackage {
          inherit pname;
          version = packageVersion;
          src = rustSource;
          cargoHash = "sha256-IAZ1G9cvxQDPrOxJVRi+civOBflAjcgnfIRbRY95i2g=";
          inherit nativeBuildInputs;
          buildInputs = fltkInputs;
          cargoBuildFlags = [
            "--workspace"
            "--no-default-features"
            "--features"
            "gui"
          ];
          cargoTestFlags = [
            "--workspace"
            "--no-default-features"
            "--features"
            "gui"
          ];

          postInstall = ''
            target_dir="target/${pkgs.stdenv.hostPlatform.config}/release"
            if [ ! -d "$target_dir" ]; then
              target_dir="target/release"
            fi

            install -Dm644 data/${desktopFile} \
              $out/share/applications/${desktopFile}
            install -Dm644 data/${appId}.metainfo.xml \
              $out/share/metainfo/${appId}.metainfo.xml
            install -Dm644 ${./packaging/appimage/io.github.hearthstone_linux_gui.svg} \
              $out/share/icons/hicolor/scalable/apps/${iconFile}
            install -Dm644 assets/client.config.in \
              $out/share/hearthstone-linux-gui/client.config.in
            install -Dm755 "$target_dir/libCoreFoundation.so" \
              $out/share/hearthstone-linux-gui/stubs/CoreFoundation.so
            install -Dm755 "$target_dir/libOSXWindowManagement.so" \
              $out/share/hearthstone-linux-gui/stubs/libOSXWindowManagement.so
            install -Dm755 "$target_dir/libblz_commerce_sdk_plugin.so" \
              $out/share/hearthstone-linux-gui/stubs/libblz_commerce_sdk_plugin.so
            install -Dm755 "$target_dir/libcommerce_http_client.so" \
              $out/share/hearthstone-linux-gui/stubs/libcommerce_http_client.so
            install -Dm755 "$target_dir/libNativeApiMac.so" \
              $out/share/hearthstone-linux-gui/stubs/libNativeApiMac.so
          '';

          postFixup = ''
            wrapProgram $out/bin/hearthstone-linux-gui \
              --set-default HEARTHSTONE_LINUX_RESOURCES "$out/share/hearthstone-linux-gui" \
              --set-default HEARTHSTONE_LINUX_STUBS "$out/share/hearthstone-linux-gui/stubs" \
              --set-default HEARTHSTONE_LINUX_RUNNER "${hearthstoneRuntime}/bin/hearthstone-linux-gui-runtime"
          '';
        };
      in
      {
        packages.default = hearthstonePackage;
        packages.hearthstone-linux-gui = hearthstonePackage;
        packages.runtime = hearthstoneRuntime;

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;
          buildInputs = fltkInputs;
          packages = with pkgs; [
            appimage-run
            hearthstoneRuntime
            rust-analyzer
          ];
          HEARTHSTONE_LINUX_RUNNER = "${hearthstoneRuntime}/bin/hearthstone-linux-gui-runtime";
          RUST_BACKTRACE = "1";
        };

        apps.default = flake-utils.lib.mkApp {
          drv = hearthstonePackage;
        };
      }
    );
}
