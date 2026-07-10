# Review: Record D-Bus Latency for systemd-Probed Services

## Spec Reference

`.github/docs/subagent_docs/systemd_probe_latency_spec.md`

## Modified Files

- `crates/vexboard-server/src/probe/uptime.rs`

## Change Summary

In `probe_systemd_unit()`:
- Added `let start = Instant::now();` immediately before `unit_active_state(&unit_name).await`, and computed
  `latency_ms` from `start.elapsed().as_millis() as i64` regardless of whether the D-Bus call succeeded or
  failed (matches spec step 1).
- Replaced `.bind(None::<i64>)` in the `probe_results` insert with `.bind(latency_ms)`.
- Replaced `latency_ms: None` in the `ProbeEvent` construction with `latency_ms: Some(latency_ms)`.

## Assessment

1. **Specification Compliance** — Matches the spec exactly: latency measured around the same D-Bus call
   `unit_active_state()` already performed, recorded on both success and failure branches, persisted and
   broadcast consistently. No schema/API/frontend changes were made, as the spec identified none were needed.
2. **Best Practices** — Follows the exact `Instant::now()` / `elapsed().as_millis() as i64` pattern already
   established in `probe_service()` in the same file — no new pattern introduced.
3. **Consistency** — `Option<i64>` shape preserved throughout (insert bind, `ProbeEvent.latency_ms`), matching
   how `probe_service()` handles latency.
4. **Completeness** — All three touch points identified in the spec (insert bind, `ProbeEvent`) were updated;
   confirmed no other `latency_ms: None` / `None::<i64>` reference remains in `probe_systemd_unit()`.
5. **Performance** — Negligible; `Instant::now()` calls are ~free, no additional D-Bus round trips added.
6. **Security** — No new attack surface; no user input involved in the changed code path.
7. **Maintainability** — Change is minimal and localized; no comments needed beyond existing code clarity.
8. **API Currency** — N/A, no external library API usage changed.

## Build Validation (commands approved in Phase 1 spec)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass (no output, no diff) |
| `cargo clippy --workspace -- -D warnings` | Pass — 0 warnings |
| `cargo test -p vexboard-server` | Pass — 34 passed; 0 failed; 0 ignored |
| `cargo build --release --bin vexboard-server` | Pass — release binary built successfully in 1m 24s |
| `cargo audit --ignore RUSTSEC-2023-0071` | Pass — exit code 0. One informational warning (RUSTSEC-2026-0190, unsoundness in `Error::downcast_mut()`) surfaced via a pre-existing `anyhow`/`wit-bindgen` dependency chain (zbus/leptos), unrelated to this change and not introduced by it. |

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

## Result

**PASS** — no CRITICAL or RECOMMENDED issues found. Proceeding directly to Phase 6 (Preflight); Phase 4/5
refinement cycle not needed.
