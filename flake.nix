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

        # Build a custom rustPlatform from the toolchain above so that
        # rustPlatform.buildRustPackage (used by package.nix) compiles with the
        # wasm32-unknown-unknown target available.
        customRustPlatform = pkgs.makeRustPlatform {
          rustc = rustToolchain;
          cargo = rustToolchain;
        };

        # Pin wasm-bindgen-cli to match the version in Cargo.lock (0.2.121).
        # Trunk enforces a hard version match between the CLI binary and the
        # wasm-bindgen crate; a mismatch aborts the build.
        #
        # HOW TO GET THE HASHES:
        #   Run `nix build` once — it will fail with "got: sha256-..." for each
        #   fakeHash below. Replace the placeholders with the values from those
        #   error messages, then re-run `nix build`.
        # Pre-fetch Swagger UI zip so utoipa-swagger-ui's build script can use it
        # from the Nix store (no network access in the sandbox).
        swaggerUiZip = pkgs.fetchurl {
          url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip";
          hash = "sha256-SBJE0IEgl7Efuu73n3HZQrFxYX+cn5UU5jrL4T5xzNw=";
        };

        wasmBindgenCli = pkgs.rustPlatform.buildRustPackage rec {
          pname = "wasm-bindgen-cli";
          version = "0.2.121";

          src = pkgs.fetchCrate {
            inherit pname version;
            hash = "sha256-ZOMgFNOcGkO66Jz/Z83eoIu+DIzo3Z/vq6Z5g6BDY/w=";
          };

          cargoHash = "sha256-DPdCDPTAPBrbqLUqnCwQu1dePs9lGg85JCJOCIr9qjU=";

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
          ];

          doCheck = false;
        };
      in
      {
        packages.vexboard = pkgs.callPackage ./nix/package.nix {
          inherit rustToolchain wasmBindgenCli swaggerUiZip;
          rustPlatform = customRustPlatform;
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

      # Apply this overlay to make pkgs.vexboard available, which the NixOS
      # module requires. Add it to nixpkgs.overlays in your NixOS configuration:
      #
      #   nixpkgs.overlays = [ inputs.vexboard.overlays.default ];
      #
      overlays.default = final: prev: {
        vexboard = self.packages.${prev.system}.vexboard;
      };
    };
}
