# Live Service-Status SSE Stream — Review (FEAT-1)

Spec: `live_service_status_sse_spec.md`

## Modified Files

- `crates/vexboard-server/src/api/services.rs` — added `stream_service_events`
  handler (SSE, mirrors `metrics_stream`) and registered `GET /stream` in
  `read_router()`
- `crates/vexboard-server/src/api/openapi.rs` — registered the new path
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` — added
  `ProbeEventFe` (wasm32-only), `live_status` signal, an `EventSource`-based
  `Effect` subscription (mirrors `MetricBar`), and a merge of live overrides into
  each card's `ServiceData` at render time
- `crates/vexboard-frontend/src/pages/dashboard/modals.rs` — removed the
  sleep-then-refetch hack in the service-create flow and its now-unused
  `gloo_timers::future::TimeoutFuture` import

## Review Against Spec

1. **Specification compliance** — implements all three spec elements: backend SSE
   endpoint, frontend subscription with live merge, and removal of the polling
   hack it superseded.
2. **Best practices** — backend handler is a near-exact mirror of the already-proven
   `metrics_stream` (same `BroadcastStream`/`filter_map`/`KeepAlive` shape); frontend
   subscription mirrors the already-proven `MetricBar` pattern (`Effect` +
   `EventSource` + `Closure` + named-event listener). No new architectural pattern
   introduced — reuses what's already validated in this codebase twice over.
3. **Consistency** — `ProbeEventFe` cfg-gated to `wasm32` only (avoids a
   never-constructed dead-code warning on native clippy checks, caught and fixed
   during this pass — see Build Validation); route registration follows the
   existing `read_router()`/`admin_router()` split (viewer-protected, matching the
   master plan's explicit instruction).
4. **Completeness** — every service on the dashboard now gets live status/latency
   updates, not just the one just created; the old fixed-1.5s guess is gone.
5. **Performance** — the render closure re-runs the full grid rebuild on any probe
   event (same cost class as an existing `services.refetch()`), an explicit,
   documented tradeoff in the spec rather than a per-card fine-grained-signal
   rewrite; acceptable for this project's scale.
6. **Security** — endpoint is viewer-protected like every other read route under
   `/api/v1/services`; no new data exposed beyond what `ProbeEvent` already carries
   (service id/name/url/status/latency — all already visible via the existing list
   endpoint for authorized viewers).
7. **API currency** — `axum::response::sse::{Event, KeepAlive, Sse}`,
   `tokio_stream::wrappers::BroadcastStream`, and `web_sys::EventSource` are all
   pre-existing, already-in-use APIs in this codebase; no new dependency, no
   Context7 lookup required per CLAUDE.md's exemption.

## Deviation From Initial Draft (caught during Phase 3)

`ProbeEventFe`, as first written, was defined unconditionally but only referenced
inside the `#[cfg(target_arch = "wasm32")]`-gated `Effect` block, so
`cargo clippy --workspace` failed with `struct 'ProbeEventFe' is never constructed`
on the native (server) build. Fixed by cfg-gating the struct declaration itself to
`wasm32` as well, matching the gating already applied to the block that uses it.
Re-ran clippy after the fix — clean.

## Build Validation (verbatim)

**`cargo build --bin vexboard-server`** (ad-hoc, backend-only sanity check before
touching the frontend) — succeeded on first try.

**`cargo fmt --all -- --check`** — clean, no diff.

**`cargo clippy --workspace -- -D warnings`** — one error caught and fixed (see
above); clean on re-run:
```
    Checking vexboard-frontend v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.02s
```

**`cargo test -p vexboard-server`**
```
running 34 tests
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
No new test added for `stream_service_events` — consistent with the existing
`metrics_stream` endpoint, which also has no test coverage (SSE handlers aren't
covered by this project's `oneshot`-based test harness today); not a gap introduced
by this change.

**`cargo build --release --bin vexboard-server`**
```
    Finished `release` profile [optimized] target(s) in 9.63s
```

**Not run:** `trunk build` (FORBIDDEN COMMANDS); no live browser verification of the
SSE round-trip was possible in this environment. The backend half (endpoint
existence, correct event framing) is verified by code review against the proven
`metrics_stream` pattern; the frontend half compiles and type-checks cleanly via
clippy but was not exercised in a running browser.

## Score Table

| Category                  | Score | Grade |
|----------------------------|-------|-------|
| Specification Compliance   | 100%  | A     |
| Best Practices              | 100%  | A     |
| Functionality                | 95%   | A     |
| Code Quality                 | 100%  | A     |
| Security                     | 100%  | A     |
| Performance                  | 90%   | A-    |
| Consistency                   | 100%  | A     |
| Build Success                 | 100%  | A     |

**Overall Grade: A (98%)**

(Functionality docked slightly only because the SSE round-trip could not be
exercised end-to-end in a live browser in this environment — code-level review
gives high confidence but isn't equivalent to an observed live test.)

## Result

**PASS** — proceeding to Phase 6 (Preflight; already run above, exit code 0).
