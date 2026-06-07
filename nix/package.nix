{ lib, rustPlatform, rustToolchain, pkg-config, openssl, dbus, linux-pam
, trunk, wasmBindgenCli, binaryen, curl, swaggerUiZip }:

let
  src = lib.cleanSource ./..;
in
rustPlatform.buildRustPackage {
  pname = "vexboard";
  version = "0.1.0";
  inherit src;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = [
    rustToolchain  # must come first to expose wasm32-unknown-unknown target to trunk
    pkg-config
    trunk
    wasmBindgenCli
    binaryen        # provides wasm-opt; prevents trunk from downloading it (no network in sandbox)
    curl            # needed by utoipa-swagger-ui build script to copy the pre-fetched zip
  ];

  # Point utoipa-swagger-ui's build script to the pre-fetched zip so it doesn't
  # attempt a network download (blocked by the Nix sandbox).
  SWAGGER_UI_DOWNLOAD_URL = "file://${swaggerUiZip}";

  buildInputs = [
    openssl
    dbus
    linux-pam
  ];

  buildPhase = ''
    # Trunk writes cache/config files; give it a writable home in the sandbox.
    export HOME=$(mktemp -d)

    # Tell trunk to use the system wasm-opt (from binaryen) rather than downloading its own copy.
    export TRUNK_TOOLS_WASM_OPT_VERSION=skip

    # Build WASM frontend
    cd crates/vexboard-frontend
    trunk build --release
    cd ../..

    # Build backend (pam-auth is Linux-only; meta.platforms enforces this)
    cargo build --release --bin vexboard-server --features pam-auth
  '';

  installPhase = ''
    mkdir -p $out/bin $out/share/vexboard/assets
    cp target/release/vexboard-server $out/bin/
    cp -r crates/vexboard-frontend/dist/* $out/share/vexboard/assets/
  '';

  meta = with lib; {
    description = "VexBoard — self-hosted server dashboard for NixOS";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
