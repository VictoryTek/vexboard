# test_probe_client_no_native_certs — Spec

## Current state
- `crates/vexboard-server/src/tests.rs:93` builds the test app's `probe_client` with `reqwest::Client::new()`.
- Workspace `reqwest` dependency (`Cargo.toml:18`) is configured `features = ["json", "rustls", "rustls-native-certs"], default-features = false`.
- Per reqwest 0.13 source (`async_impl/client.rs`, verified via Context7 `/seanmonstar/reqwest`), when the native-roots feature is active, `ClientBuilder::build()` eagerly calls `rustls_native_certs::load_native_certs()` at construction time — before any request is sent — and returns `Err(General("No CA certificates were loaded from the system"))` if the OS store yields zero valid certs.
- Production code (`crates/vexboard-server/src/main.rs:196-198`) already builds its `probe_client` via `.build()?`, so a real cert-load failure surfaces as a normal startup error — correct behavior for a server that genuinely dials HTTPS targets.
- The Nix sandbox used for `cargo test` has no populated `/etc/ssl/certs`, so every test that calls `TestApp::new()` panics on `Client::new()` before the test body runs. This affects 13 of 34 tests in `vexboard-server` and fails the release build.
- None of the 13 failing tests (`test_login_success`, `test_health_check`, `test_create_service_as_admin`, etc.) issue real outbound HTTPS requests — they all exercise the Axum `Router` in-process via `tower::ServiceExt`/`axum::body`. The `probe_client` field is only ever read by the probe endpoint handler in `services.rs:278`, which is not invoked by these tests. So the test client's TLS trust store is dead weight for every failing test.

## Problem
Test-only `reqwest::Client::new()` eagerly loads OS CA certs even though tests never perform real TLS handshakes, causing spurious build failures in cert-less sandboxes (Nix build, likely also affects minimal Docker/CI images without a cert bundle).

## Solution
In `crates/vexboard-server/src/tests.rs`, replace:
```rust
probe_client: reqwest::Client::new(),
```
with:
```rust
probe_client: reqwest::Client::builder()
    .tls_certs_only(std::iter::empty())
    .build()
    .unwrap(),
```
`tls_certs_only(certs)` (verified against the actual installed `reqwest` 0.13.1 source at `~/.cargo/registry/src/.../reqwest-0.13.1/src/async_impl/client.rs` — the Context7-returned `tls_built_in_native_certs` signature did not match this crate version and does not compile) disables native/built-in root loading and uses only the provided (empty) certificate set, so `build()` never calls `rustls_native_certs::load_native_certs()` and succeeds regardless of sandbox environment. Since this client is never used to dial a real TLS endpoint in the test suite, an empty trust store has no behavioral effect on any test.

No change to `main.rs` — production must keep failing loudly if the real OS cert store is unusable, since it actually probes HTTPS services.

## Implementation steps
1. Edit `crates/vexboard-server/src/tests.rs:93` as above.
2. Run `cargo test -p vexboard-server` to confirm all 13 previously-failing tests now pass and no regressions in the other 21.
3. Run `cargo fmt --all -- --check` and `cargo clippy --workspace -- -D warnings`.
4. Run `cargo build --release --bin vexboard-server` to confirm release build compiles (matches the failing Nix build command).

## Dependencies
None — `tls_built_in_native_certs` is a method on `reqwest::ClientBuilder` already available under the existing `rustls`/`rustls-native-certs` feature set; no Cargo.toml feature or dependency change required. Verified against current reqwest 0.13 API via Context7 (`/seanmonstar/reqwest`).

## Configuration changes
None.

## Risks / mitigations
- Risk: a future test starts making real HTTPS requests through `probe_client` and silently gets an empty trust store. Mitigation: none needed today — this is the same latent risk as any test double; if that happens, the resulting connection failure will be an obvious, loud test failure, not a silent pass.
- Risk: divergence between test and production client construction. Mitigation: acceptable — the two serve different purposes (in-process router testing vs. real network probing), and production's real cert-loading behavior is exactly what should NOT be short-circuited.
