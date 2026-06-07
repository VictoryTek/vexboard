# Spec: Nix Flake Build Fixes

## Current State Analysis

`flake.nix` defines:
- `packages.vexboard` via `nix/package.nix`
- `devShells.default` with Rust + Trunk + sqlx-cli
- `nixosModules.vexboard` via `nix/module.nix`

`nix/package.nix` uses `rustPlatform.buildRustPackage` and calls `trunk build --release`
inside the `buildPhase`.

## Problems Identified

### Problem 1: wasm-bindgen-cli placeholder hashes
- `flake.nix` used `lib.fakeHash` / placeholder strings for `wasm-bindgen-cli` 0.2.121
- `hash` and `cargoHash` placeholders cause `nix build` to fail until replaced with real values
- **Fix:** Replace placeholders with the real SRI hashes obtained from the first failed build

### Problem 2: utoipa-swagger-ui downloads Swagger UI at build time
- `utoipa-swagger-ui` 9.0.2 build script curls
  `https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip`
- Network access is blocked in the Nix sandbox — build fails
- **Fix:**
  - Add `swaggerUiZip = pkgs.fetchurl { ... }` fixed-output derivation in `flake.nix`
  - Pass `swaggerUiZip` to `pkgs.callPackage ./nix/package.nix { ... }`
  - In `package.nix`: add `curl` to `nativeBuildInputs` and set
    `SWAGGER_UI_DOWNLOAD_URL = "file://${swaggerUiZip}";` so the build script
    reads from the Nix store instead of the network

### Problem 3 (prior): trunk sandbox issues — RESOLVED
- Trunk writes cache to `$HOME`; fixed via `export HOME=$(mktemp -d)`
- Trunk downloads `wasm-opt`; fixed via `binaryen` in nativeBuildInputs + `TRUNK_TOOLS_WASM_OPT_VERSION=skip`

## Proposed Solution

### flake.nix
1. Replace `hash` placeholder with `sha256-ZOMgFNOcGkO66Jz/Z83eoIu+DIzo3Z/vq6Z5g6BDY/w=`
2. Replace `cargoHash` placeholder with `sha256-DPdCDPTAPBrbqLUqnCwQu1dePs9lGg85JCJOCIr9qjU=`
3. Add `swaggerUiZip = pkgs.fetchurl { url = "...v5.17.14.zip"; hash = "sha256-SBJE0IEgl7Efuu73n3HZQrFxYX+cn5UU5jrL4T5xzNw="; };`
4. Pass `swaggerUiZip` in `inherit` to `callPackage`

### nix/package.nix
1. Add `curl` and `swaggerUiZip` to function arguments
2. Add `curl` to `nativeBuildInputs`
3. Set `SWAGGER_UI_DOWNLOAD_URL = "file://${swaggerUiZip}";` as top-level attribute

## Implementation Steps
1. Edit `flake.nix`: fix hashes, add swaggerUiZip fetchurl, pass through callPackage
2. Edit `nix/package.nix`: add curl + swaggerUiZip args, add curl to nativeBuildInputs, set env var

## Dependencies
- `pkgs.fetchurl` — fixed-output derivation; available in nixpkgs
- `pkgs.curl` — curl CLI; available in nixpkgs; needed by utoipa-swagger-ui build script

## Build/Test Commands (Phase 3)
- `cargo fmt --all -- --check` — zero resource cost
- `cargo clippy --workspace -- -D warnings` — lint only
- `cargo test --workspace` — server-side tests only (frontend excluded)
- `cargo build --release --bin vexboard-server` — backend binary only
- `nix build` — full Nix derivation build (required by user; validates both fixes end-to-end)

## Risks
- `nix build` compiles the entire project + WASM frontend; time and disk intensive
- If the swaggerUiZip hash is wrong, Nix will report the correct hash in the error
- `curl` must be available in `nativeBuildInputs` for the build script's file:// URL copy
