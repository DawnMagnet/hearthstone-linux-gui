{
  description = "Native Rust GTK4/libadwaita Hearthstone Linux manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };
        nativeBuildInputs = with pkgs; [
          gnumake
          cargo
          desktop-file-utils
          glib
          gobject-introspection
          gtk4
          libadwaita
          pkg-config
          rustToolchain
          wrapGAppsHook4
        ];
        buildInputs = with pkgs; [
          cairo
          gdk-pixbuf
          glib
          graphene
          gtk4
          libadwaita
          openssl
          pango
        ];
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "hearthstone-linux";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          inherit nativeBuildInputs buildInputs;
          buildFeatures = [ "gui" ];

          preBuild = ''
            make -C stubs
          '';

          postInstall = ''
            install -Dm644 data/io.github.hearthstone_linux.desktop \
              $out/share/applications/io.github.hearthstone_linux.desktop
            install -Dm644 data/io.github.hearthstone_linux.metainfo.xml \
              $out/share/metainfo/io.github.hearthstone_linux.metainfo.xml
            install -Dm644 assets/client.config.in \
              $out/share/hearthstone-linux/client.config.in
            install -Dm755 stubs/CoreFoundation.so \
              $out/share/hearthstone-linux/stubs/CoreFoundation.so || true
            install -Dm755 stubs/libOSXWindowManagement.so \
              $out/share/hearthstone-linux/stubs/libOSXWindowManagement.so || true
            install -Dm755 stubs/libblz_commerce_sdk_plugin.so \
              $out/share/hearthstone-linux/stubs/libblz_commerce_sdk_plugin.so || true
          '';
        };

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          packages = with pkgs; [
            appimage-run
            cryptopp
            gcc
            linuxdeploy
            rust-analyzer
          ];
          RUST_BACKTRACE = "1";
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };
      });
}
