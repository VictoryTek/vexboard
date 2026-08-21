# Uptime History & Incident Timeline — Review

## Files changed

Backend:
- `crates/vexboard-server/src/config.rs` — `ProbeConfig::max_history: u64` → `history_retention_days: u64`
- `config/default.toml` — key renamed, default 30 (days)
- `crates/vexboard-server/src/tests.rs` — fixture updated
- `README.md` — architecture-flow mention updated
- `crates/vexboard-server/src/probe/uptime.rs` — age-based `prune_old_results`
  replacing the row-count `NOT IN (SELECT ... LIMIT ?)` subquery in both
  probe functions; new `compute_uptime_summary`/`derive_incidents` pure
  functions + 7 unit tests
- `crates/vexboard-server/src/probe/mod.rs` — scheduler passes `history_retention_days`
- `crates/vexboard-server/src/api/services.rs` — renamed call-site variable;
  new `GET /{id}/uptime` route + `service_uptime_summary` handler; `MAX_SUMMARY_ROWS` safety cap
- `crates/vexboard-server/src/db/models.rs` — new `Incident`, `UptimeSummary` structs
- `crates/vexboard-server/src/api/openapi.rs` — new path + 2 schemas registered

Frontend:
- `crates/vexboard-frontend/src/components/service_card.rs` — history strip
  now clickable via new `on_history: Option<Callback<i64>>` prop
- `crates/vexboard-frontend/src/components/history_modal.rs` — new `HistoryModal` component
- `crates/vexboard-frontend/src/components/mod.rs` — module registered
- `crates/vexboard-frontend/src/pages/dashboard/mod.rs` — new `history_target` signal, threaded through
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` — `on_history` wired
- `crates/vexboard-frontend/src/pages/dashboard/group_section.rs` — `on_history` wired
- `crates/vexboard-frontend/src/pages/dashboard/modals.rs` — renders `<HistoryModal>`
- `crates/vexboard-frontend/style/main.css` — new `.history-*` block

## Review checklist

1. **Specification compliance** — all of spec sections 3a/3b/3c implemented
   as written; section 4's explicit exclusions (no detail page, card
   sparkline untouched, no Settings-UI retention control) honored — verified
   `components/service_card.rs::history_strip` and its `/history?limit=N`
   endpoint are byte-for-byte unchanged except for the new click wrapper. ✅
2. **Best practices** — derivation logic is a pure, deterministic function
   taking `now` as a parameter (not reading the clock internally), matching
   the spec's explicit testability requirement; colocated with its own
   `#[cfg(test)] mod tests`, mirroring `discovery::systemd`'s established
   convention in this codebase. ✅
3. **Consistency** — `HistoryModal` follows the exact overlay/backdrop/panel
   markup already used by `EditModal`/`GroupsModal` (inline styles, no new
   modal CSS system); `history_target: RwSignal<Option<(i64, String)>>`
   follows the exact `edit_target`/`edit_link_target` tuple-signal
   convention already threaded through `ServiceGrid`/`GroupSection`/`DashboardModals`.
   Corrected an initial over-application of `#[cfg(target_arch = "wasm32")]`
   in `history_modal.rs` (gloo_net/spawn_local calls in this codebase are
   NOT gated — only genuine `web_sys`/DOM APIs are, per `dashboard/mod.rs`
   and `group_section.rs` precedent) before finalizing. ✅
4. **Maintainability** — new CSS block is self-contained (`.history-*`
   namespace, existing color tokens only); no duplication with the
   settings-page CSS block from the prior facelift. ✅
5. **Completeness** — 24h/7d/30d percentages, heartbeat bar, incident list
   with duration/ongoing-state all present; empty states handled (no data →
   `—`, no incidents → explicit message). ✅
6. **Performance** — one query per modal open (capped at `MAX_SUMMARY_ROWS
   = 20,000`), one query per probe tick for pruning (replaced a
   `NOT IN (subquery)` with a plain indexed-column range delete — cheaper,
   not more expensive). No new polling. ✅
7. **Security** — new endpoint reuses the same route-group auth middleware
   as every other `/api/v1/services/*` read route (no new auth code
   written); viewing history is intentionally not admin-gated, matching the
   spec's explicit call that this is a read action. ✅
8. **API currency** — no external library involved; no Context7 lookup needed. ✅
9. **Build validation** — see below.

## Build validation (verbatim)

**`cargo fmt --all -- --check`** — clean on first run after this feature's changes.

**`cargo clippy --workspace -- -D warnings`** — clean, both crates, zero
warnings, including the new `history_modal.rs` and `probe/uptime.rs` derivation code.

**`cargo test -p vexboard-server`**
```
test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
48 pre-existing + 7 new (`probe::uptime::tests::*`, covering: no incidents
when all-up, a resolved incident's exact duration, an ongoing incident's
duration-to-now, `down`+`unknown` merging into one incident, most-recent-first
ordering, `None` uptime percentage when a window has zero data, and the
50-row heartbeat cap).

**`cargo build --release --bin vexboard-server`** — succeeded.

## WASM/Trunk build — not run

Same environment limitation as the settings facelift: no `wasm32-unknown-unknown`
target and no Trunk on PATH here, so `trunk build`/browser verification of
the new modal was not performed in this session (forbidden without both
confirmed present). Native fmt/clippy/test/build all pass; the modal's
actual on-screen behavior (heartbeat bar rendering, click-to-open, close-on-backdrop)
should be spot-checked in a browser once Trunk is available.

## Score table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 90% | A- (not verified in-browser) |
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
[PASS] cargo test          (55 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
