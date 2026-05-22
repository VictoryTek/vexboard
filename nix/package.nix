{ lib, stdenv, rustPlatform, rustToolchain, pkg-config, openssl, dbus, linux-pam, trunk, wasm-bindgen-cli }:

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
    pkg-config
    trunk
    wasm-bindgen-cli
  ];

  buildInputs = [
    openssl
    dbus
    linux-pam
  ];

  # Build frontend first, then backend
  buildPhase = ''
    # Build WASM frontend
    cd crates/vexboard-frontend
    trunk build --release
    cd ../..

    # Build backend
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
