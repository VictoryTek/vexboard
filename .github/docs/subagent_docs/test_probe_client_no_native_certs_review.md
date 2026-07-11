# test_probe_client_no_native_certs — Review

## Scope
- `crates/vexboard-server/src/tests.rs` — test-only `probe_client` construction, no eager native CA cert loading.
- `Cargo.toml` — workspace version drift fix (`0.1.1` → `0.2.0`, matching the `v0.2.0` git tag already 15 commits behind HEAD).
- `Cargo.lock` — regenerated for `vexboard-server`/`vexboard-frontend` entries to match (`cargo update --offline -p vexboard-server -p vexboard-frontend`, no dependency changes, offline/metadata-only).

## Deviation from spec
Spec originally proposed `.tls_built_in_native_certs(false)`, sourced from a Context7 doc snippet. That method does not exist on the actually-installed `reqwest` 0.13.1 (`ClientBuilder` in `~/.cargo/registry/.../reqwest-0.13.1/src/async_impl/client.rs` has no such method — compile error `E0599`). Verified the real API directly against the vendored source and switched to `.tls_certs_only(std::iter::empty())`, which is present in this exact crate version, disables native/built-in root loading, and uses the (empty) provided set. Spec file updated to reflect this. Functionally equivalent outcome (no eager native cert load), just a different, version-correct method name.

## Checks

1. **Specification Compliance** — Implementation matches the (corrected) spec exactly; scope stayed limited to the test helper, production `main.rs` untouched.
2. **Best Practices** — Uses the documented, non-deprecated reqwest 0.13.1 API (`add_root_certificate` is explicitly marked deprecated in favor of `tls_certs_merge`/`tls_certs_only`).
3. **Consistency** — Matches existing builder-pattern usage already used for `probe_client` and `notify_client` in `main.rs`.
4. **Maintainability** — Self-explanatory; no comment needed since the field name and builder call are unambiguous.
5. **Completeness** — All previously-failing tests addressed; version drift addressed at both `Cargo.toml` and `Cargo.lock`.
6. **Performance** — No effect (test-only code path).
7. **Security** — No production TLS behavior changed; production probe client and its `?`-propagated startup failure on missing certs are untouched.
8. **API Currency** — Confirmed against actual installed crate source after Context7 doc mismatch; not the deprecated `add_root_certificate`.
9. **Build Validation** — see below, all commands from the approved list, verbatim output.

### `cargo fmt --all -- --check`
Exit 0, no output (clean).

### `cargo clippy --workspace -- -D warnings`
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
```
Exit 0.

### `cargo test -p vexboard-server` (with `SQLX_OFFLINE=true`)
```
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
All 13 previously-failing tests now pass; all previously-passing tests still pass. Exit 0.

### `cargo build --release --bin vexboard-server` (with `SQLX_OFFLINE=true`)
```
Finished `release` profile [optimized] target(s) in 11.65s
```
Exit 0. Binary reports version `0.2.0` (workspace version now matches `Cargo.toml`).

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 95% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (99%)**

(Spec compliance marked 95% only because the exact method name in the original spec had to be corrected against real crate source — the intent and outcome were fully preserved.)

## Result
**PASS** — no CRITICAL or RECOMMENDED issues outstanding. Proceeding to Phase 6 (Preflight); Phase 4/5 refinement cycle not required.
