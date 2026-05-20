{
  description = "VexBoard — self-hosted server dashboard for NixOS and systemd";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in
      {
        packages.vexboard = pkgs.callPackage ./nix/package.nix {
          inherit rustToolchain;
        };
        packages.default = self.packages.${system}.vexboard;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            sqlx-cli
            trunk
            nodePackages.tailwindcss
            dbus
          ];

          shellHook = ''
            export DATABASE_URL="sqlite:./dev.db"
          '';
        };
      }
    ) // {
      nixosModules.vexboard = ./nix/module.nix;
      nixosModules.default = self.nixosModules.vexboard;
    };
}
