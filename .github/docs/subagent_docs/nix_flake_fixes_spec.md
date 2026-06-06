# Spec: Nix Flake Build Fixes

## Current State Analysis

`flake.nix` defines:
- `packages.vexboard` via `nix/package.nix`
- `devShells.default` with Rust + Trunk + sqlx-cli
- `nixosModules.vexboard` via `nix/module.nix`

`nix/package.nix` uses `rustPlatform.buildRustPackage` and calls `trunk build --release`
inside the `buildPhase`.

## Problems Identified

### Problem 1: wasm-bindgen-cli version mismatch
- `Cargo.lock` pins `wasm-bindgen` at **0.2.121**
- `package.nix` takes `wasm-bindgen-cli` from `pkgs` (nixpkgs-unstable), which may be a
  different version
- Trunk enforces that the installed `wasm-bindgen-cli` binary matches the version in Cargo.lock;
  a mismatch aborts the build with a hard error
- **Fix:** Override `wasm-bindgen-cli` in the flake to build from source at exactly 0.2.121,
  using `pkgs.rustPlatform.buildRustPackage` with `fetchCrate`. Hash placeholders use
  `lib.fakeHash`; user fills in real hashes from the first `nix build` error message.

### Problem 2: trunk build sandbox issues
- Nix derivation builds run with `HOME` unset or pointing to a read-only location
- Trunk writes a cache/config to `$HOME/.local/share/trunk` or `$XDG_CACHE_HOME/trunk`
- Trunk by default attempts to download `wasm-opt` at build time; network is blocked in sandbox
- **Fix:**
  - Set `export HOME=$(mktemp -d)` at the start of `buildPhase`
  - Add `pkgs.binaryen` (provides `wasm-opt`) to `nativeBuildInputs`
  - Set `TRUNK_TOOLS_WASM_OPT_VERSION=skip` so trunk uses the system binary rather than
    downloading; wasm-opt is still provided via nativeBuildInputs for actual optimization

### Problem 3: pam-auth feature unconditionally enabled (minor)
- `package.nix` passes `--features pam-auth` unconditionally
- `linux-pam` is in buildInputs, so it compiles, but it should be gated on Linux-only platforms
  explicitly in the meta
- **Fix:** Add `linux-pam` guard and note; `platforms = platforms.linux` is already set, so
  this is acceptable but should be documented

### Non-issue: SQLx offline cache
- Project uses `sqlx::query(...)` runtime calls only — no `query!` macros
- No `.sqlx/` directory or `SQLX_OFFLINE=true` is required
- Confirmed by grepping: 0 occurrences of `query!`, `query_as!`, `query_scalar!` macros

## Proposed Solution

### nix/package.nix
- Accept `wasmBindgenCli` as a parameter (renamed from `wasm-bindgen-cli` for clarity)
- Add `binaryen` to `nativeBuildInputs`
- Set `HOME`, `TRUNK_TOOLS_WASM_OPT_VERSION=skip` in `buildPhase`

### flake.nix
- Define a local `wasmBindgenCli` derivation pinned to 0.2.121 using `pkgs.rustPlatform.buildRustPackage`
- Pass it explicitly to `callPackage ./nix/package.nix { wasmBindgenCli = wasmBindgenCli; }`
- Add `binaryen` to `devShells.default` buildInputs

## Implementation Steps
1. Edit `flake.nix`: add `wasmBindgenCli` overlay derivation, pass to `callPackage`, add `binaryen` to devShell
2. Edit `nix/package.nix`: rename param, add `binaryen` to nativeBuildInputs, fix buildPhase env

## Dependencies
- `pkgs.binaryen` — provides `wasm-opt`; available in nixpkgs-unstable
- `pkgs.rustPlatform.buildRustPackage` — already used; no new flake inputs needed

## Build/Test Commands (Phase 3)
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- Nix syntax check: `nix flake check --no-build` (validates expression without building)

## Risks
- `lib.fakeHash` placeholders in `wasmBindgenCli` will cause `nix build` to fail on first run
  with the correct hash in the error message — this is expected Nix workflow
- If nixpkgs-unstable already ships 0.2.121, the override is still correct (same hash)
- `TRUNK_TOOLS_WASM_OPT_VERSION=skip` tells trunk to skip its own wasm-opt download;
  the actual optimization still runs via the system `wasm-opt` from `binaryen`
