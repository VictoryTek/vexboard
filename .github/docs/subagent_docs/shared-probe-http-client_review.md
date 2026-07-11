# BUG-6 — Per-Probe `reqwest::Client` + Timeout-less Fallback — Review

## Summary

Implementation matches spec across all five touched files:

- `main.rs`: one `reqwest::Client` built once at startup with `config.probe.timeout_secs` baked
  in, propagated via `?` (fatal on build failure, replacing the old silent `unwrap_or_default()`
  fallback pattern that this fix specifically targets). Added `probe_client: reqwest::Client` to
  `AppState`, populated in the state literal, and cloned into the probe-loop spawn.
- `probe/mod.rs`: `start_probe_loop` now takes `client: reqwest::Client`; the per-service spawn
  clones it (mirroring the existing `db`/`tx` clone pattern) instead of building a fresh client
  each cycle; the now-unused `let timeout = ...` line was removed.
- `probe/uptime.rs`: `probe_service` now takes `client: &reqwest::Client` instead of
  `timeout: Duration`; the internal `Client::builder()...unwrap_or_default()` block is gone
  entirely; the now-unused `Duration` import was removed (`Instant` alone remains, still used).
- `api/services.rs`: `create_service`'s immediate-probe task now clones `state.probe_client`
  instead of computing `timeout_secs` and building a `Duration`; the `std::time::Duration` import
  is retained since it's still used elsewhere in the file (SSE keep-alive interval).
- `tests.rs`: `AppState` test literal updated with `probe_client: reqwest::Client::new()` — a
  mechanical, in-scope consequence of the new struct field (test-only client, timeout
  irrelevant since no test exercises live probing).

All three call sites (`probe/mod.rs`, `api/services.rs`'s inline probe, and the removed internal
build in `uptime.rs`) now route through a single client, and startup failure to build that
client now correctly aborts server startup instead of silently producing a timeout-less client.

## Build & Test Results (verbatim)

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --workspace -- -D warnings`:
```
    Checking vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.79s
```
Exit 0, no warnings. (An initial pass caught an unused `Duration` import in `uptime.rs`, fixed
before this final run — confirms clippy's unused-import lint correctly surfaced the removed
client-builder code's now-dangling import.)

`cargo test -p vexboard-server`:
```
running 34 tests
...
test tests::test_create_service_as_admin ... ok
test tests::test_create_and_delete_service_as_admin ... ok
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```
Exit 0. `test_create_service_as_admin` / `test_create_and_delete_service_as_admin` exercise the
`create_service` handler that spawns the immediate-probe task now using `state.probe_client`,
confirming the new field wiring compiles and runs without panicking.

`cargo build --release --bin vexboard-server`:
```
    Compiling vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `release` profile [optimized] target(s) in 11.43s
```
Exit 0.

## Review Against Criteria

1. **Specification Compliance** — exact match to spec across all five files.
2. **Best Practices** — mirrors the codebase's own established pattern (`notify_client` in
   `main.rs`); `reqwest::Client` reuse is the crate's documented recommended usage.
3. **Consistency** — the `client.clone()` pattern in `probe/mod.rs`'s spawn matches the
   pre-existing `db.clone()`/`tx.clone()` style immediately adjacent to it.
4. **Maintainability** — removes a per-call client-construction block entirely rather than
   parameterizing it further; the shared-client threading is straightforward parameter passing.
5. **Completeness** — both call sites of `probe_service` (`probe/mod.rs` scheduler and
   `services.rs` immediate post-create probe) updated identically; no stray internal
   client-building code left behind.
6. **Performance** — directly addresses the stated inefficiency: one persistent connection
   pool/TLS state reused across all probes instead of rebuilt per call.
7. **Security** — closes the fail-open timeout gap: a `Client::builder().build()` failure now
   aborts startup instead of silently yielding a client with no timeout enforcement (previously
   a potential probe-task hang / resource exhaustion vector under a rare build failure).
8. **API Currency** — no deprecated `reqwest` API usage introduced; `Client::builder()` and
   `.clone()` are current, standard API surface.
9. **Build Validation** — all four approved commands run clean (see above).

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (100%)**

## Returns

- Build result: PASS (fmt, clippy, tests, release build all clean)
- **PASS**
