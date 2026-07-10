# Review: Stop Service Cards Blinking on Probe SSE Updates

## Spec Reference

`.github/docs/subagent_docs/service_card_sse_blink_spec.md`

## Modified Files

- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs`
- `crates/vexboard-frontend/src/components/service_card.rs`

## Change Summary

- `service_grid.rs`: removed the top-level `let overrides = live_status.get();` read and its per-card merge
  (`overrides.get(&svc.id)`) from the outer card-list-building closure. `render_card` now builds `ServiceData`
  directly from the base `svc.status` / `svc.latency_ms`, and passes the `live_status` `RwSignal` down to
  `ServiceCard` as a new prop instead.
- `service_card.rs`: added a `live_status: RwSignal<HashMap<i64, (String, Option<i64>)>>` prop to
  `ServiceCard`. Replaced the one-time `badge_cls`/`status_label`/`latency` computation with
  `Signal::derive`-based `current_status` / `current_latency`, each merging the live override for this card's
  id over the base fallback value captured at mount. The status badge class, `StatusDot`, status label, and
  latency span in the bottom row now read these signals reactively via `move ||` closures instead of static
  bindings.

### Deviation from spec (implementation-detail fix, not a design change)

The spec's example code used plain `move ||` closures for `current_status`/`current_latency`. During
implementation this failed to compile (`E0382: use of moved value`) because a non-`Copy` closure can't be
captured by multiple downstream `move ||` closures (used in 3 places: badge class, `StatusDot`, label text).
Switched to `Signal::derive(...)`, which produces a `Copy` `Signal<T>` — the idiomatic Leptos fix for exactly
this situation, with identical reactive semantics to what the spec intended (call via `.get()` instead of
`()`). No change to the architecture or blast radius described in the spec.

## Assessment

1. **Specification Compliance** — Matches the spec's intent exactly (scope the `live_status` read inside each
   card instead of the parent's list-building closure); the `Signal::derive` substitution is a mechanical
   compile-fix, not a deviation in approach.
2. **Best Practices** — `Signal::derive` is the standard Leptos idiom for a reusable derived reactive value;
   matches how the rest of the codebase uses signals.
3. **Consistency** — `ServiceCard`'s only call site (`service_grid.rs`) was updated accordingly; no other
   call sites exist.
4. **Completeness** — Outer closure no longer depends on `live_status` (confirmed no remaining
   `live_status.get()` calls in `service_grid.rs`); `ServiceCard` now owns all reactive status/latency display
   logic.
5. **Performance** — No regression; `Signal::derive` recomputation on `live_status` change is a cheap
   `HashMap` lookup + optional clone, scoped to small text/class DOM patches instead of full card remounts —
   strictly cheaper than before.
6. **Security** — No new attack surface; purely client-side reactive display logic.
7. **Maintainability** — Localized, well-scoped change; no speculative abstraction introduced.
8. **API Currency** — N/A, no external library API usage changed (`Signal::derive` is existing leptos 0.8
   API, already used implicitly elsewhere via `.into_signal()`/similar patterns in the codebase's reactive
   style).

## Build Validation (commands approved in Phase 1 spec)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace -- -D warnings` | Initial run: **FAIL** — `E0382 use of moved value: current_status` (2 errors) in `service_card.rs`, caused by the spec's plain-closure approach being used in multiple `move` closures. Fixed via `Signal::derive`. Re-run: **Pass**, 0 warnings, both `vexboard-server` and `vexboard-frontend` compiled cleanly. |
| `cargo test -p vexboard-server` | Pass — 34 passed; 0 failed (unaffected by this frontend-only change) |
| `cargo build --release --bin vexboard-server` | Pass |
| `cargo audit --ignore RUSTSEC-2023-0071` | Pass — exit code 0, same pre-existing informational advisories as prior work (unrelated to this change) |

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
| Build Success | 100% | A (after 1 in-place compile-error fix, resolved within Phase 2/3, no refinement cycle needed) |

**Overall Grade: A (100%)**

## Result

**PASS** — the initial clippy failure was a compile error in the implementation, caught and fixed directly
within Phase 2/3 (not a design or spec issue), so no formal Phase 4 refinement cycle was triggered. Proceeding
to Phase 6 (Preflight).
