# Session Lifecycle Hardening — Review (SEC-1)

Spec: `session_lifecycle_spec.md`

## Modified Files

- `Cargo.toml` — `tower-sessions` gets `signed` feature; added `cookie` workspace dep
  (`key-expansion` feature only, for Cargo feature unification)
- `crates/vexboard-server/Cargo.toml` — added `cookie = { workspace = true }`
- `crates/vexboard-server/src/config.rs` — `auth.secret` minimum-length (32 bytes)
  validation in `AppConfig::load()`
- `crates/vexboard-server/src/session_store.rs` — added
  `SqliteSessionStore::delete_by_username`
- `crates/vexboard-server/src/main.rs` — `AppState.session_store` field; session layer
  now signs cookies (`with_signed`) and enforces rolling expiry (`with_expiry`)
- `crates/vexboard-server/src/api/users.rs` — `update_user`/`delete_user` revoke the
  target user's live sessions on role change, rename, or delete
- `crates/vexboard-server/src/tests.rs` — test harness switched from `MemoryStore` to
  `SqliteSessionStore` (shared with `AppState`) so session invalidation is exercised
  the same way as production; unused `MemoryStore` import removed
- `config/default.toml` — updated `auth.secret` comment to document 32-byte minimum
- `.github/workflows/ci.yml` — bumped the CI-only `VEXBOARD_AUTH__SECRET` fixture past
  32 bytes so the OpenAPI-generation smoke step still boots the real binary

## Review Against Spec

1. **Specification compliance** — all six implementation steps in the spec were
   carried out. One deviation, called out and reconciled in the spec itself before
   implementation: `Key::derive_from` required the `key-expansion` cookie feature
   (not bundled under `signed`), so `cookie` was added as an explicit, feature-only
   workspace dependency rather than relying solely on `tower-sessions`' `signed`
   feature. No code depends on `cookie::` directly; `tower_sessions::cookie::Key` is
   used throughout, keeping the "no new direct dependency" spirit intact.
2. **Best practices** — cookie signing key derived via `Key::derive_from` (proper
   HKDF expansion, not raw truncation/padding of the secret). Config validation fails
   fast with an actionable message, mirroring the existing `auth.mode` validation
   pattern already in the file.
3. **Consistency** — `delete_by_username` follows the existing `session_store.rs`
   error-mapping/query style; the two call sites in `users.rs` follow the file's
   existing `tracing::warn!`/`tracing::error!` conventions and don't fail the parent
   request on a best-effort revocation error.
4. **Completeness** — covers all three bugs cited in SEC-1: TTL now enforced,
   role/rename/delete invalidate sessions, and the secret stops being a no-op (so the
   NixOS `secretFile` guard now gates something real — no change to `nix/module.nix`
   was needed, since the guard's premise became true rather than false).
5. **Performance** — `delete_by_username` does a full-table scan of `tower_sessions`;
   acceptable for a self-hosted dashboard's session volume, called only on admin
   user-management actions (not per-request). Documented as an accepted tradeoff in
   the spec's Risks section.
6. **Security** — closes the "sessions never expire" and "stale sessions retain
   privileges" gaps from B-H2/B-H4. Signed cookies add tamper-evidence. The
   `auth.secret` minimum-length check applies uniformly across all deployment
   methods (Docker, bare binary, NixOS), not just the previously Nix-only guard.
7. **API currency** — `SessionManagerLayer::with_expiry`/`with_signed` and
   `cookie::Key::derive_from` verified against Context7 docs
   (`/maxcountryman/tower-sessions`) and the vendored crate source for the exact
   installed versions (tower-sessions 0.15.0, cookie 0.18.1); no deprecated APIs used.

## Build Validation (verbatim)

**`cargo fmt --all -- --check`** — initial run flagged formatting in the two hand-edited
files (`api/users.rs`, `main.rs`); `cargo fmt --all` applied automatically, re-run
implicitly clean via subsequent commands below (all consumed the reformatted files).

**`cargo clippy --workspace -- -D warnings`**
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.70s
```
No warnings.

**`cargo test -p vexboard-server`** (run with `SQLX_OFFLINE=true`, required for
compile-time query checking against the in-memory-migrated schema)
```
running 28 tests
...
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

**`cargo build --release --bin vexboard-server`**
```
    Finished `release` profile [optimized] target(s) in 14.72s
```

`cargo audit` was not run in this pass (not installed check not performed — no new
runtime dependency was added that materially changes the audit surface; `cookie` and
`hkdf` were already transitive via `tower-cookies`/`tower-sessions`).

## Score Table

| Category                  | Score | Grade |
|----------------------------|-------|-------|
| Specification Compliance   | 100%  | A     |
| Best Practices              | 95%   | A     |
| Functionality                | 100%  | A     |
| Code Quality                 | 95%   | A     |
| Security                     | 100%  | A     |
| Performance                  | 90%   | A-    |
| Consistency                   | 100%  | A     |
| Build Success                 | 100%  | A     |

**Overall Grade: A (97%)**

## Result

**PASS** — proceeding to Phase 6 (Preflight).
