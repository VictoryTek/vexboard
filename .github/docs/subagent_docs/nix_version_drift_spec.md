# nix_version_drift — Spec

## Current state
- `Cargo.toml` `[workspace.package].version` was bumped to `0.1.1` in commit f840157.
- `nix/package.nix:9` hardcoded `version = "0.1.0";` in the `buildRustPackage` derivation — never updated, so `nix build` / `nixos-rebuild switch` report `vexboard-0.1.0` regardless of the actual crate version.
- Two other locations independently hardcoded `0.1.0` as literal strings with no link back to `Cargo.toml`:
  - `crates/vexboard-server/src/api/openapi.rs:21` — `#[openapi(info(version = "0.1.0", ...))]` (utoipa OpenAPI doc metadata).
  - `crates/vexboard-frontend/src/pages/settings.rs:199` — UI "About" footer text `"VexBoard v0.1.0 — ..."`.
- Both `vexboard-server` and `vexboard-frontend` crates already use `version.workspace = true` in their `Cargo.toml`, so they inherit `0.1.1` at the Cargo level; the drift was only in code/Nix that didn't read that value.

## Problem
Version string duplicated in 3 independent places (Nix derivation, OpenAPI doc, UI footer) with no single source of truth, guaranteeing drift on every release bump.

## Solution
1. `nix/package.nix`: replace the literal with `(builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version`, evaluated at Nix eval time — no rebuild-time cost, always in sync with the checked-out `Cargo.toml`.
2. `crates/vexboard-server/src/api/openapi.rs`: replace the literal with `env!("CARGO_PKG_VERSION")`, a compile-time Cargo-provided env var that resolves through `version.workspace = true` to `0.1.1`.
3. `crates/vexboard-frontend/src/pages/settings.rs`: replace the literal with `concat!("VexBoard v", env!("CARGO_PKG_VERSION"), " — Self-hosted server dashboard for NixOS and systemd.")`, same mechanism, compile-time constant usable directly in the Leptos `view!` macro.

## Dependencies
None — both `env!`/`concat!` are `std` macros already available; no new crates or Nix inputs.

## Risks / mitigations
- `builtins.fromTOML` requires a Nix version supporting TOML parsing (available since Nix 2.4+, already required by flake usage elsewhere in this repo) — no new constraint introduced.
- `env!("CARGO_PKG_VERSION")` requires the crate be built via `cargo build`/`buildRustPackage` (always true here) — not applicable to non-Cargo build paths.
