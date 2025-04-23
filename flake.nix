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
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux"];
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
            ];

            nativeBuildInputs = with pkgs; [
              rustPlatform.bindgenHook
              pkg-config
              wrapGAppsHook4
            ];
          };
      };
    };
}
