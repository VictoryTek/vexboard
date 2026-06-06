# Spec: Nix Flake — VexOS & Any-Flakes Integration Fixes

## Current State Analysis

`flake.nix` exposes:
- `packages.{system}.vexboard` — package derivation (currently broken: rustToolchain not used)
- `devShells.{system}.default` — dev environment
- `nixosModules.{vexboard,default}` — NixOS module

`nix/module.nix` provides `services.vexboard.*` options. Critical issues:
- References `pkgs.vexboard` in ExecStart but there is no overlay exposing that package
- Serves the binary without setting the assets path, so the UI never loads
- `settings` option is declared but generates no config file
- No `package` option — can't override which derivation is used

`nix/package.nix` builds the binary + WASM bundle. Critical issues:
- Accepts `rustToolchain` as a parameter but never uses it — `rustPlatform` comes from
  the default nixpkgs (no `wasm32-unknown-unknown` target), so `trunk build` will fail
  when cross-compiling to WASM
- Accepts `stdenv` (auto-resolved by callPackage) but never uses it

`crates/vexboard-server/src/config.rs` load order:
  `env vars > /etc/vexboard/config.toml > config/default.toml`

`crates/vexboard-server/src/main.rs:136-140`:
  When `assets_path == "embedded"`, falls back to relative `"assets"` directory.
  In the NixOS service, CWD has no `assets/` dir — UI would 404 on all static assets.

---

## Problem Definitions

### P1 — No `overlays.default` (CRITICAL)
`nixosModules.vexboard` uses `pkgs.vexboard` in `ExecStart`. No overlay exposes this
package. Any NixOS configuration importing this module without adding the overlay manually
will fail to evaluate. VexOS integration requires `overlays.default`.

### P2 — `rustToolchain` unused in `package.nix` (CRITICAL)
`rustPlatform.buildRustPackage` uses the nixpkgs-default `rustPlatform`, which does not
include the `wasm32-unknown-unknown` target. `trunk build --release` calls
`cargo build --target wasm32-unknown-unknown`; this fails if rustc doesn't have the target.
The fix is to create a custom `rustPlatform` from the `rustToolchain` (which does have the
WASM target) and use it in `package.nix`.

### P3 — Assets path not configured in NixOS module (CRITICAL)
The systemd service has no `VEXBOARD_SERVER__ASSETS_PATH` env var. The server falls back
to `./assets` relative to CWD (no such directory). Frontend WASM, JS, CSS, and HTML
are installed to `${package}/share/vexboard/assets/` but the server never looks there.

### P4 — `settings` option is a declared no-op (MEDIUM)
`module.nix` declares `settings = lib.mkOption { type = lib.types.attrs; }` with
description "Extra settings merged into config.toml", but never generates a config file.
The server reads `/etc/vexboard/config.toml` if present. Wire `settings` to generate that
file using `pkgs.formats.toml {}` and expose it via `environment.etc`.

### P5 — No `package` option in module (MEDIUM)
NixOS module best practice: expose a `package` option. Allows VexOS or any consumer to
pin a specific derivation without forking the module. Default: `pkgs.vexboard` (resolved
by the overlay).

### P6 — No auth secret management (LOW)
`config/default.toml` has `secret = "change-me-in-production"`. The module sets no
override. Add a `secretFile` option: when set, `EnvironmentFiles` loads its content as
env vars (expected: `VEXBOARD_AUTH__SECRET=...`). When unset, document that the user must
supply `VEXBOARD_AUTH__SECRET` via another mechanism.

---

## Proposed Solution

### flake.nix changes
1. Add `customRustPlatform = pkgs.makeRustPlatform { rustc = rustToolchain; cargo = rustToolchain; }`
2. Pass `customRustPlatform` to `callPackage` as `rustPlatform`
3. Add `overlays.default = final: prev: { vexboard = self.packages.${prev.system}.vexboard; };`
   outside of `eachDefaultSystem` (overlays are system-independent)

### nix/package.nix changes
1. Remove unused `stdenv` parameter (callPackage injects it automatically; unused)
2. Change function signature to receive `rustPlatform` (will now be the custom one)
3. `rustToolchain` stays in signature to be added to `nativeBuildInputs`; this ensures
   the custom rustc (with WASM target) shadows the platform default in PATH during
   the Trunk build phase

### nix/module.nix changes
1. Add `package` option: `lib.mkOption { type = lib.types.package; default = pkgs.vexboard; }`
2. Replace `${pkgs.vexboard}` with `${cfg.package}` in ExecStart
3. Add `VEXBOARD_SERVER__ASSETS_PATH` env var pointing to `${cfg.package}/share/vexboard/assets`
4. Wire `settings` to generate `/etc/vexboard/config.toml` using `pkgs.formats.toml {}`:
   - Base config: server.{host, port, assets_path}, database.path — derived from module options
   - `lib.recursiveUpdate baseConfig cfg.settings` for user overrides
   - Expose via `environment.etc."vexboard/config.toml".source = configFile`
   - Remove env var redundancy where config file covers the same fields
5. Add `secretFile` option (nullable path). When set, add to `serviceConfig.EnvironmentFiles`.
   Document expected format: `VEXBOARD_AUTH__SECRET=<secret>` on its own line.
6. Remove the now-redundant `host`/`port`/`dataDir` env vars from Environment list
   (they are in the generated config file; keep only non-config-file env vars)

---

## Implementation Plan

### Step 1: flake.nix
- Add `customRustPlatform` in `let` block
- Pass it to `callPackage`
- Add `overlays` output after `eachDefaultSystem` block

### Step 2: nix/package.nix
- Update function signature
- Add `rustToolchain` to `nativeBuildInputs`

### Step 3: nix/module.nix
- Full rewrite of config wiring

---

## Build/Test Commands (Phase 3)
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `nix flake check --no-build`

## Risks
- `pkgs.formats.toml {}` may not serialize all Nix types correctly for complex `settings`
  attrs; add a type note in the option description to guide users
- `overlays.default` introduces a system-dependent lookup; use
  `builtins.currentSystem` or ensure flake consumers apply the overlay with their own
  system — the standard `nixpkgs.overlays` mechanism handles this correctly
- Auth secret file is optional; without it, the default placeholder secret is used —
  document clearly that production deployments must supply a secret

## Files Modified
- `flake.nix`
- `nix/package.nix`
- `nix/module.nix`
