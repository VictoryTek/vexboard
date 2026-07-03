# pam_build_warnings — Spec

## Current State Analysis

A `pam-auth`-feature build (`cargo build --release --features pam-auth`, as
run by the Nix package) emits 6 warnings, all `unused_imports` / `dead_code`.
Root cause confirmed by inspection: `pam-auth` is optional and default-off
(`Cargo.toml`: `default = []`, `pam-auth = ["dep:pam-sys"]`). When it is
enabled, every local-username/password (non-PAM) code path in
`crates/vexboard-server/src/api/auth.rs` and
`crates/vexboard-server/src/api/setup.rs` is compiled out via
`#[cfg(not(all(unix, feature = "pam-auth")))]` / `#[cfg(not(feature = "pam-auth"))]`.
The items that only exist to serve those compiled-out paths become dead code
for that build configuration:

1. `setup.rs:1` — `use axum::{extract::State, ...}`: `State` only used by
   the non-pam `status`/`create_admin` fns (gated `#[cfg(not(feature = "pam-auth"))]`).
2. `setup.rs:5` — `use crate::db;`: only used inside non-pam `create_admin`.
3. `setup.rs:6` — `use crate::AppState;`: only used as the `State<AppState>`
   extractor type in the non-pam fns.
4. `auth.rs:38` — `struct UpdateMeRequest`: only constructed inside the
   non-pam `update_me` (gated `#[cfg(not(all(unix, feature = "pam-auth")))]`,
   auth.rs:313). No other reference anywhere in the crate (confirmed —
   no `UpdateMeRequest` schema entry in `openapi.rs`).
5. `setup.rs:10-11` — `SetupRequest.username`/`.password` fields: only
   *read* inside non-pam `create_admin`. **However**, `SetupRequest` itself
   is referenced unconditionally by `openapi.rs:77`
   (`components(schemas(... crate::api::setup::SetupRequest ...))`), so the
   struct and its fields cannot be removed or fully cfg-gated without
   breaking the OpenAPI schema in pam-auth builds.
6. `db/models.rs:34` — `struct User`: its only consumer,
   `db::users::get_user_by_username`, is entirely gated
   `#[cfg(not(all(unix, feature = "pam-auth")))]` (confirmed via crate-wide
   search — no other file references `models::User`). Not referenced by
   `openapi.rs`.

## Problem Definition

Fix all 6 warnings without changing runtime behavior in either build
configuration (pam-auth on or off), without removing functionality needed
by the non-pam build, and without breaking the OpenAPI schema.

## Proposed Solution

- **setup.rs imports**: split the `axum` import so `extract::State` is only
  pulled in under `#[cfg(not(feature = "pam-auth"))]`; gate
  `use crate::db;` and `use crate::AppState;` the same way, matching the
  existing per-function cfg gates in the same file.
- **auth.rs `UpdateMeRequest`**: gate the struct definition with
  `#[cfg(not(all(unix, feature = "pam-auth")))]`, matching the cfg already
  used on its sole consumer (the non-pam `update_me`, auth.rs:313) and the
  file's existing convention (e.g. auth.rs:77, 132, 257, 295).
- **db/models.rs `User`**: gate the struct definition with
  `#[cfg(not(all(unix, feature = "pam-auth")))]`, matching the cfg on its
  sole consumer `db::users::get_user_by_username`.
- **setup.rs `SetupRequest` fields**: cannot be cfg-gated (struct is used
  unconditionally by `openapi.rs` for schema generation). Add
  `#[cfg_attr(feature = "pam-auth", allow(dead_code))]` on the struct to
  suppress the warning only in the build configuration where the fields are
  genuinely unread, while keeping the struct and its fields intact and
  documented for the non-pam build and for OpenAPI schema purposes in both.

No other files change. No new dependencies — Context7 lookup not required
(internal-only cfg/attribute change, no external library involved).

## Implementation Steps

1. `crates/vexboard-server/src/api/setup.rs`: adjust imports (split `State`
   out of the `axum::{...}` group under its own `#[cfg(not(feature = "pam-auth"))]`
   `use`; add the same cfg to `use crate::db;` and `use crate::AppState;`);
   add `#[cfg_attr(feature = "pam-auth", allow(dead_code))]` above
   `pub struct SetupRequest`.
2. `crates/vexboard-server/src/api/auth.rs`: add
   `#[cfg(not(all(unix, feature = "pam-auth")))]` above
   `pub(crate) struct UpdateMeRequest`.
3. `crates/vexboard-server/src/db/models.rs`: add
   `#[cfg(not(all(unix, feature = "pam-auth")))]` above `pub struct User`.

## Dependencies

None (no new crates; no Context7 lookup needed — pure `cfg`/attribute
changes to existing internal code).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk**: Gating `User` or `UpdateMeRequest` incorrectly could break the
  default (non-pam-auth) build. **Mitigation**: cfg conditions exactly
  mirror the cfg already used on each struct's sole consumer function, so
  default-feature builds are unaffected.
- **Risk**: `cargo build --release --bin vexboard-server` in Phase 3/6 review
  compiles the *default* feature set (pam-auth off), so it will not
  regenerate the pam-auth warnings and cannot directly prove the fix. This
  is inherent to the FORBIDDEN COMMANDS / resource constraints (no bare
  `cargo build --features pam-auth` validation step is defined for this
  project, and `pam-auth` requires `libpam-dev` on Linux which may not be
  installed). Mitigation: verify via `cargo check --bin vexboard-server
  --features pam-auth` if `libpam-dev` is available; otherwise reason
  about correctness from the cfg symmetry with existing, already-compiling
  gated code (same pattern used at auth.rs:77/132/257/295 today).
</content>
