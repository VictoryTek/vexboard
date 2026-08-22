# Service Detail: Live Logs — Review

## Files changed

Backend:
- `crates/vexboard-server/src/control/systemd.rs` — `ChildLogStream` (owns
  the `journalctl -f` child so `kill_on_drop` fires on stream drop) + `tail_unit_logs`
- `crates/vexboard-server/src/control/docker.rs` — `tail_container_logs`
  via bollard's native `Docker::logs(...)` streaming API
- `crates/vexboard-server/src/api/services.rs` — `BoxedLogStream` type
  (type-erases the two backends into one SSE response type),
  `to_sse_log_stream` mapper, new admin-only `GET /{id}/logs/stream` route
  + handler (same server-side-lookup safety pattern as `control_service`)
- `crates/vexboard-server/src/api/openapi.rs` — path registered
- `crates/vexboard-server/src/tests.rs` — 2 new tests
- `crates/vexboard-server/Cargo.toml` — `tokio-stream` gains the `io-util` feature

Frontend:
- `crates/vexboard-frontend/src/components/history_modal.rs` — admin-only
  "Show/Hide Logs" toggle + scrolling panel, `EventSource`-based live tail
  with explicit connection teardown and a 500-line client-side cap
- `crates/vexboard-frontend/style/main.css` — `.history-logs`, `.history-log-line`

## Review checklist

1. **Specification compliance** — matches the spec's narrowed scope
   exactly: logs only (resource stats/search/download explicitly deferred),
   admin-only (stricter than Feature 1's history view, as specced), off by
   default (opening the modal doesn't start a stream — only clicking
   "Show Logs" does), explicit teardown on toggle-off/modal-close/target-change. ✅
2. **Best practices** — subprocess lifetime is tied to the stream's own
   `Drop` via `kill_on_drop(true)` and an owned `_child` field, rather than
   a manual process-tracking table; both backends produce the same
   `Result<Event, Infallible>` item type via one small mapper
   (`to_sse_log_stream`) instead of duplicating SSE-framing logic per backend. ✅
3. **Consistency** — reuses the exact server-side service-lookup pattern
   from `control_service` (client sends only an id, server resolves
   unit/container, never trusts a name from the request); reuses the same
   socket-matching logic already established for control actions; frontend
   `EventSource` wiring follows `pages/dashboard/mod.rs`'s existing
   probe-stream precedent for what needs `#[cfg(target_arch = "wasm32")]`
   (raw `web_sys`/`Closure` calls) versus what doesn't (`gloo_net`,
   `spawn_local` elsewhere in this same file are deliberately left ungated,
   per Feature 1's established finding) — with one intentional difference,
   noted in its own comment: this connection is opened and closed
   *repeatedly* as an admin toggles it, so the handle is held in a `RefCell`
   captured once by a single long-lived effect closure and explicitly
   `.close()`d, rather than the dashboard listener's `.forget()`-for-app-lifetime
   pattern, which would leak a connection on every toggle here. ✅
4. **Maintainability** — `BoxedLogStream`/`to_sse_log_stream` is the one
   place a third log backend would plug in later. ✅
5. **Completeness** — manual (URL-only) services rejected with the same
   400 message pattern as Feature 2's control routes; spawn/socket
   failures return a real 502 with the underlying error, not a silently
   broken stream. ✅
6. **Performance** — no polling; the stream only exists while the modal's
   Logs panel is open; client caps retained lines to 500 so a long-open
   panel doesn't grow memory unboundedly. ✅
7. **Security** — admin-only, no read tier at all (log output is arbitrary
   process text, treated with the same caution as Feature 3 treated
   channel `target` URLs). Server-side-only unit/container resolution
   closes the same injection surface Feature 2 already closed for control actions. ✅
8. **API currency** — bollard's `logs()` Context7-checked; docs resolved
   to the newer `query_parameters` builder API (0.20) again, exactly as
   they did for Feature 2's control calls against this workspace's pinned
   0.17 — used the older `LogsOptions::<String> { ..Default::default() }`
   struct-literal shape already proven in `discovery/docker.rs`, confirmed
   correct by a clean `cargo check`. ✅
9. **Build validation** — see below, including a meaningful gap closed this round.

## Build validation (verbatim)

**`cargo fmt --all -- --check`** — clean.

**`cargo clippy --workspace -- -D warnings`** (native target) — clean, both
crates, zero warnings.

**New this round — wasm32 target verification.** Every prior feature's
`#[cfg(target_arch = "wasm32")]` code (including this one's new
`EventSource` logic) had only ever been *reviewed*, never *type-checked*,
because native-target `cargo check`/`clippy` skip that `cfg` entirely and
no `wasm32-unknown-unknown` target was installed in this environment.
Added it this round (`rustup target add wasm32-unknown-unknown` — not a
forbidden command, distinct from the Trunk/wasm-bindgen toolchain that
actually is gated) and ran:

```
cargo check --target wasm32-unknown-unknown -p vexboard-frontend    → clean
cargo clippy --target wasm32-unknown-unknown -p vexboard-frontend -- -D warnings → clean
```

This closes most of the "WASM/Trunk build — not run" caveat repeated in
every prior review: the wasm-only code (this feature's `EventSource`
open/close logic, and retroactively every `#[cfg(target_arch = "wasm32")]`
block from Features 1–3) is now confirmed to actually compile for its real
target, not just plausible-looking on paper. What's still unverified is
runtime browser behavior — actual DOM events, real SSE frames, the
auto-scroll-on-new-line behavior — which needs Trunk + wasm-bindgen-cli
and a browser, neither available here.

**`cargo test -p vexboard-server`**
```
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
60 pre-existing + 2 new (404 for an unknown service, 400 for a manual
service — the same pre-condition-logic testing boundary used for the
control routes in Feature 2; the actual `journalctl`/bollard streaming
paths aren't exercised by automated tests, consistent with this
codebase's established boundary around D-Bus/Docker-touching code).

**`cargo build --release --bin vexboard-server`** — succeeded.

## WASM/Trunk build — narrowed, not closed

Type-checking (`cargo check`/`clippy --target wasm32-unknown-unknown`) now
passes, a real step up from every prior review. A full `trunk build` and
in-browser exercise of the toggle/stream/auto-scroll/cleanup behavior is
still not done — Trunk and wasm-bindgen-cli aren't installed here, and
running them without confirming that remains against project policy.

## Score table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 92% | A- (wasm32 type-checked this round; still not browser-verified) |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 97% | A (native + wasm32 type-checks pass; full Trunk/browser build unverified) |

**Overall Grade: A (98%)**

## Result: **PASS**

No CRITICAL issues found.

## Phase 6 — Preflight

`scripts/preflight.ps1` executed directly:

```
[PASS] cargo fmt
[PASS] cargo clippy
[PASS] cargo test          (62 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
