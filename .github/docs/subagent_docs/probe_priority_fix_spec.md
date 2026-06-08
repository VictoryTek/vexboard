# Phase 1 Spec — probe_priority_fix

**Date:** 2026-06-07

## Current State Analysis

The probe dispatcher in two places uses URL-first priority:

**`crates/vexboard-server/src/probe/mod.rs` (lines 39–43):**
```rust
if svc.url.is_some() {
    uptime::probe_service(&db, &svc, timeout, max_history, &tx).await;
} else if svc.systemd_unit.is_some() {
    uptime::probe_systemd_unit(&db, &svc, max_history, &tx).await;
}
```

**`crates/vexboard-server/src/api/services.rs` (lines 175–186):**
```rust
if svc.url.is_some() {
    probe::uptime::probe_service(...)
} else if svc.systemd_unit.is_some() {
    probe::uptime::probe_systemd_unit(...)
}
```

## Problem Definition

Services discovered via systemd (e.g. Sonarr, Radarr, Lidarr, Prowlarr) are registered
in the DB with both `systemd_unit` AND `url` set. The URL is the web UI address.

Because `url.is_some()` is checked first, these services are always HTTP-probed.

The HTTP probe fails because:
- The *arr services require authentication → return 401 Unauthorized
- 401 is not `is_success()` or `is_redirection()` → recorded as "down"

Result: the dashboard shows DOWN for services that systemd reports as active.

## Proposed Solution

Invert the probe priority: check `systemd_unit` first, fall back to URL only when no
systemd unit is configured.

```rust
if svc.systemd_unit.is_some() {
    probe_systemd_unit(...)
} else if svc.url.is_some() {
    probe_service(...)
}
```

This ensures:
- Services managed by systemd always report their actual systemd active-state
- Pure HTTP services (no systemd_unit) continue to be HTTP-probed as before
- No new dependencies, no schema changes, no new fields

## Affected Files

1. `crates/vexboard-server/src/probe/mod.rs` — scheduled probe loop dispatcher
2. `crates/vexboard-server/src/api/services.rs` — immediate post-create probe dispatcher

## Implementation Steps

1. In `probe/mod.rs` lines 39–43: swap the if/else if branches
2. In `api/services.rs` lines 175–186: swap the if/else if branches

## Dependencies

No new dependencies. No Context7 lookup required — internal code change only.

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check` — zero-cost formatting check
- `cargo clippy --workspace -- -D warnings` — lint check (backend only)
- `cargo test --workspace` — unit/integration tests
- `cargo build --release --bin vexboard-server` — full backend build

All approved per CLAUDE.md. No FORBIDDEN COMMANDS used.

## Risks and Mitigations

- **Risk:** Services with a systemd_unit that is stale/wrong in the DB will now show DOWN
  even if their URL is accessible.
  **Mitigation:** This is the correct behavior — if the admin configured a systemd_unit,
  systemd state is the intended health signal. The user can clear the systemd_unit field
  to revert to HTTP-only probing.

- **Risk:** Services where D-Bus is unavailable will all show DOWN.
  **Mitigation:** Already handled — `unit_active_state` logs a warning and returns
  "inactive" on D-Bus error. No change to error handling needed.
