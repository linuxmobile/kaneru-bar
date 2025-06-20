{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    systems.url = "github:nix-systems/default-linux";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      systems = import inputs.systems;
      perSystem = {pkgs, ...}: {
        devShells.default = let
          rust-bin = inputs.rust-overlay.lib.mkRustBin {} pkgs;
        in
          pkgs.mkShell {
            packages = with pkgs; [
              (rust-bin.selectLatestNightlyWith (toolchain:
                toolchain.default.override {
                  extensions = ["rust-analyzer" "rust-src"];
                }))
              gtk4
              gtk4-layer-shell
              libadwaita
              glib
              cairo
              pango
              gdk-pixbuf
              graphene
              wayland
              adwaita-icon-theme
              dart-sass
              wlsunset
              brightnessctl
            ];

            nativeBuildInputs = with pkgs; [
              rustPlatform.bindgenHook
              pkg-config
              openssl
              wrapGAppsHook4
              glib
            ];
          };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "kaneru";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = with pkgs; [
            pkg-config
            rustPlatform.bindgenHook
            pkg-config
            openssl
            wrapGAppsHook4
            glib
            dart-sass
          ];
          buildInputs = with pkgs; [
            gtk4
            gtk4-layer-shell
            libadwaita
            glib
            cairo
            pango
            gdk-pixbuf
            graphene
            wayland
            adwaita-icon-theme
            dart-sass
            wlsunset
            brightnessctl
          ];
        };
      };
    };
}
