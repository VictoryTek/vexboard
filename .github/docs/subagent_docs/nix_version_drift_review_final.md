# nix_version_drift — Final Review

## Files changed
- `nix/package.nix` — derivation `version` now derived from `Cargo.toml` (`(builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version`) instead of hardcoded `"0.1.0"`.
- `crates/vexboard-server/src/api/openapi.rs` — OpenAPI `info(version = ...)` now `env!("CARGO_PKG_VERSION")` instead of hardcoded `"0.1.0"`.
- `crates/vexboard-frontend/src/pages/settings.rs` — About footer text now built with `concat!("VexBoard v", env!("CARGO_PKG_VERSION"), ...)` instead of a hardcoded `"VexBoard v0.1.0"` literal.

## Validation

| Category | Result |
|---|---|
| Specification Compliance | Matches spec exactly — all 3 identified drift points fixed |
| Best Practices | Uses standard Cargo (`env!`) and Nix (`builtins.fromTOML`) idioms, no new deps |
| Consistency | Both crates already had `version.workspace = true`; changes align with existing single-source-of-truth intent |
| Security | No change to attack surface |
| Build — `cargo fmt --all -- --check` | Pass (no output/diff) |
| Build — `cargo clippy -p vexboard-server -- -D warnings` | Pass, 0 warnings |
| Build — `cargo build --release --bin vexboard-server --features pam-auth` | Pass — compiled as `vexboard-server v0.1.1`, confirming `env!("CARGO_PKG_VERSION")` resolves correctly |
| Build — `cargo test -p vexboard-server` | Pass — 34/34 tests passed |
| Frontend (WASM) build | **Not run** — Trunk CLI and `wasm32-unknown-unknown` target are not installed on this machine (confirmed via `which trunk` / `rustup target list --installed`); running `trunk build` is forbidden per CLAUDE.md without confirmed toolchain presence. The `concat!`/`env!` usage is standard-library-only and syntactically valid Rust, but was not compiled for the wasm target in this pass. |
| `nix build` | **Not run** — no Nix installed on this Windows dev machine; the derivation change was verified by inspection only (standard `builtins.fromTOML` pattern, used elsewhere in the Nix ecosystem for exactly this purpose). |

## Outcome: PASS

Note for user: recommend running `nix build .#vexboard` (or `nixos-rebuild switch` against a flake pointing here) and confirming the derivation now reports `vexboard-0.1.1`, and running `trunk build --release` on a machine with the WASM toolchain to confirm the frontend footer renders "VexBoard v0.1.1" before merging, since neither could be validated in this environment.
