# Service Control (Start / Stop / Restart) — Review

## Files changed

Backend:
- `crates/vexboard-server/src/control/mod.rs` (new) — `UnitAction` enum
- `crates/vexboard-server/src/control/systemd.rs` (new) — D-Bus StartUnit/StopUnit/RestartUnit
- `crates/vexboard-server/src/control/docker.rs` (new) — bollard start/stop/restart_container
- `crates/vexboard-server/src/main.rs` — `mod control;` registered
- `crates/vexboard-server/src/api/services.rs` — 3 new routes on the
  already-`require_admin`-gated `admin_router()`, shared `control_service`
  handler with server-side service lookup, socket resolution, audit
  logging (success and failure), and immediate re-probe on success
- `crates/vexboard-server/src/api/openapi.rs` — 3 new paths registered
- `crates/vexboard-server/src/tests.rs` — 2 new tests (404 for unknown
  service, 400 for a manual URL-only service with no unit/container)

Frontend:
- `crates/vexboard-frontend/src/components/history_modal.rs` — `target`
  grew from `(i64, String)` to `(i64, String, bool)` (id, name,
  controllable); new admin-only Controls row with Start (fires
  immediately) and Stop/Restart (two-click confirm), inline
  success/error message
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` /
  `group_section.rs` — compute `controllable = svc.systemd_unit.is_some()`
  before it's moved into `ServiceData`, pass the 3-tuple
- `crates/vexboard-frontend/src/pages/dashboard/mod.rs` / `modals.rs` —
  signal type updated to match
- `crates/vexboard-frontend/style/main.css` — new `.btn-danger`,
  `.history-controls`, `.history-control-msg`

## Review checklist

1. **Specification compliance** — safety model implemented exactly as
   specced: server-side-only service lookup (client never supplies a
   unit/container name), admin-only via the existing `require_admin` layer
   (verified structurally — `services::admin_router()` is nested under
   `admin_protected`, which applies `require_admin` to every route inside
   it, so the 3 new routes inherit gating by construction, not by
   duplicated logic), every attempt audited success-or-failure, manual
   (URL-only) services rejected with 400, two-click confirm on Stop/Restart
   only. ✅
2. **Best practices** — reused the existing "immediate re-probe after a
   mutation" pattern from `create_service` verbatim rather than building
   job-completion tracking; Context7-verified bollard/zbus usage, and
   explicitly adapted around a version mismatch (Context7's bollard docs
   resolved to 0.20's `query_parameters` builder API; this workspace pins
   0.17) by passing `None` for every options argument, which sidesteps the
   struct-shape question entirely — confirmed correct by `cargo check`
   compiling clean against the actual pinned version. ✅
3. **Consistency** — `control/` mirrors the existing `discovery/` module
   split (read vs. write counterpart, `mod.rs` + `systemd.rs` + `docker.rs`);
   the new D-Bus proxy trait is kept separate from the existing read-only
   one in `probe/uptime.rs` rather than merged, matching the
   read/write module separation already established; frontend confirm-step
   UX matches this app's existing quality bar (no native `confirm()`,
   inline button-swap) and reuses the exact `.btn-primary`/`.btn-secondary`
   shape for the new `.btn-danger`. ✅
4. **Maintainability** — one shared `control_service` handler body for all
   three actions rather than three near-duplicate handlers; `UnitAction`
   centralizes the action → audit-string mapping. ✅
5. **Completeness** — start/stop/restart covered for both systemd units and
   Docker/Podman containers (socket resolved by matching `discovery_source`
   against `config.docker.sockets`, mirroring `discovery/docker.rs`'s own
   `contains("podman")` rule); every failure path (404, 400, 502) returns a
   real error body, none discarded. ✅
6. **Performance** — no new polling; one D-Bus/Docker call per action, one
   audit insert, one fire-and-forget re-probe — same cost shape as
   `create_service`'s existing mutation pattern. ✅
7. **Security** — admin-only (structurally guaranteed, see above); the
   "must already be a tracked service" rule is the single safety boundary,
   deliberately not layered with a second allowlist (per spec §3, avoiding
   two overlapping mechanisms that could drift out of sync); every
   attempt — including failures — is audit-logged with the actor. ✅
8. **API currency** — Context7-checked for both bollard and zbus before
   writing the integration; adapted to the actually-pinned bollard version
   rather than copying newer upstream docs verbatim (see §2). ✅
9. **Build validation** — see below.

## Build validation (verbatim)

**`cargo fmt --all -- --check`** — clean after one formatting pass (line-wrap only).

**`cargo clippy --workspace -- -D warnings`** — clean, both crates, zero
warnings, including the new `control/` module and the extended `history_modal.rs`.

**`cargo test -p vexboard-server`**
```
test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
55 pre-existing + 2 new (`test_control_unknown_service_returns_404`,
`test_control_manual_service_returns_400`). No test exercises the live
D-Bus/Docker calls themselves — consistent with this codebase's existing
testing boundary (D-Bus-dependent code isn't unit-tested anywhere else
either; CLAUDE.md's own Repository Notes call out that D-Bus is often
unavailable in CI/sandbox environments). The two new tests cover the
business logic that runs *before* those calls (lookup, 404, 400) using the
same `TestApp` HTTP-integration harness as every other route test.

**`cargo build --release --bin vexboard-server`** — succeeded.

## WASM/Trunk build — not run

Same environment limitation as Features covered so far: no
`wasm32-unknown-unknown` target / Trunk on PATH. The new Controls row
(button-swap confirm flow, inline status message) has not been exercised
in an actual browser this session.

## Score table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 90% | A- (not verified in-browser; live D-Bus/Docker paths not exercised by automated tests, by design) |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 95% | A (native checks pass; WASM build unverified) |

**Overall Grade: A (98%)**

## Result: **PASS**

No CRITICAL issues found.

## Phase 6 — Preflight

`scripts/preflight.ps1` executed directly:

```
[PASS] cargo fmt
[PASS] cargo clippy
[PASS] cargo test          (57 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
