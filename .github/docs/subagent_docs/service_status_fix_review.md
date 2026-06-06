# Phase 3 Review — service_status_fix

**Date:** 2026-06-06

## Problem Statement

Two distinct bugs with service status display:

1. **Status not shown immediately after adding a service** — status dot stays `"—"` (unknown) until the page is manually refreshed or cache cleared, because no probe had run by the time the frontend's first refetch completed.

2. **Systemd services never show up/down status** — `probe/mod.rs` queried `WHERE probe_enabled = 1 AND url IS NOT NULL`, permanently excluding systemd services that have no URL. `probe_service` also returned early when no URL was set, so the D-Bus path was never reached.

---

## Modified Files

| File | Change |
|------|--------|
| `crates/vexboard-server/src/probe/uptime.rs` | Added `probe_systemd_unit`, `unit_active_state`, `SystemdManager` zbus proxy, `SystemdUnitInfo` struct |
| `crates/vexboard-server/src/probe/mod.rs` | Updated probe loop query and dispatch to include systemd-only services |
| `crates/vexboard-server/src/api/services.rs` | `create_service` now spawns an immediate background probe after insert |
| `crates/vexboard-frontend/src/pages/dashboard/modals.rs` | Added 1.5 s delayed second refetch after creating a service |
| `crates/vexboard-frontend/Cargo.toml` | Enabled `futures` feature on `gloo-timers` |

---

## Review Criteria

### 1. Specification Compliance — 100% / A

All four root causes identified and addressed. No scope creep; no new features.

### 2. Best Practices — 95% / A

- `probe_systemd_unit` follows the same pattern as `probe_service`: instrument with `#[tracing::instrument]`, write to `probe_results`, trim history, broadcast event.
- `unit_active_state` is a private helper — correctly not exported.
- Active-state mapping (`active | reloading | activating` → `"up"`) matches systemd semantics.
- Errors from D-Bus (unavailable in some environments) log a warning and return `"down"` — graceful degradation.
- Minor: `list_units()` fetches all loaded units to find one. Acceptable for probe cadence (called every N seconds per service, not per request); a `GetUnit` + property read would be marginally more efficient but adds proxy complexity.

### 3. Functionality — 100% / A

- Immediate probe fires in a `tokio::spawn` — does not block the 201 response.
- Probe is gated on `probe_enabled = 1` in the SQL query.
- Frontend delayed refetch (1 500 ms) is sufficient for local D-Bus probes (< 100 ms) and typical LAN HTTP probes.
- Systemd services now included in the recurring probe loop, not just on create.

### 4. Code Quality — 95% / A

- All formatting passes `cargo fmt --all -- --check`.
- All lint passes `cargo clippy --workspace -- -D warnings`.
- No commented-out code or dead variables.
- Probe dispatch in `mod.rs` (`url.is_some()` → HTTP, else `systemd_unit.is_some()` → D-Bus) is readable and consistent with the existing probe architecture.

### 5. Security — 100% / A

- D-Bus connection uses `Connection::system()` — connects to the system bus, same as the discovery module. No privilege escalation.
- No user-supplied input reaches the D-Bus query path.
- No new attack surface in the HTTP path.

### 6. Performance — 90% / A-

- `list_units()` returns all loaded units (~tens to low hundreds on a typical server). Linear scan to find one unit is O(n) but n is small and this runs in a background `tokio::spawn`, never on a hot path.
- The immediate post-create probe runs asynchronously and does not affect API response time.

### 7. Consistency — 100% / A

- `probe_systemd_unit` mirrors the structure of `probe_service` exactly (instrument, probe, DB write, trim, broadcast).
- Frontend 1.5 s delay uses `gloo_timers::future::TimeoutFuture`, consistent with the async WASM patterns already used elsewhere.
- zbus proxy in `probe/uptime.rs` mirrors the proxy in `discovery/systemd.rs`.

### 8. Build Success — 100% / A

```
cargo fmt --all -- --check  →  PASS
cargo clippy --workspace    →  PASS
cargo build --release --bin vexboard-server  →  PASS
preflight.sh exit code      →  0  (All preflight checks passed)
```

---

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 95% | A |
| Functionality | 100% | A |
| Code Quality | 95% | A |
| Security | 100% | A |
| Performance | 90% | A- |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (97.5%)**

---

## Result: PASS

No critical issues. No refinement cycle required.
