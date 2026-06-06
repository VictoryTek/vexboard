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

        # Pin wasm-bindgen-cli to match the version in Cargo.lock (0.2.121).
        # Trunk enforces a hard version match between the CLI binary and the
        # wasm-bindgen crate; a mismatch aborts the build.
        #
        # HOW TO GET THE HASHES:
        #   Run `nix build` once — it will fail with "got: sha256-..." for each
        #   fakeHash below. Replace the placeholders with the values from those
        #   error messages, then re-run `nix build`.
        wasmBindgenCli = pkgs.rustPlatform.buildRustPackage rec {
          pname = "wasm-bindgen-cli";
          version = "0.2.121";

          src = pkgs.fetchCrate {
            inherit pname version;
            hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
          };

          cargoHash = "sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
          ];

          doCheck = false;
        };
      in
      {
        packages.vexboard = pkgs.callPackage ./nix/package.nix {
          inherit rustToolchain wasmBindgenCli;
        };
        packages.default = self.packages.${system}.vexboard;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            sqlx-cli
            trunk
            wasmBindgenCli
            binaryen
            tailwindcss
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
