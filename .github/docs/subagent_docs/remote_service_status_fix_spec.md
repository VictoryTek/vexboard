# Phase 1 Spec — remote_service_status_fix

**Date:** 2026-07-02

## Current State Analysis

Probe dispatch in two locations decides HTTP vs. systemd D-Bus probing purely on field
presence, with `systemd_unit` checked first (see `probe_priority_fix_spec.md`,
2026-06-07 — this ordering was intentional, fixing *arr apps that have both
`systemd_unit` and `url` set and fail HTTP probing with 401):

**`crates/vexboard-server/src/probe/mod.rs` (lines 39–43):**
```rust
if svc.systemd_unit.is_some() {
    uptime::probe_systemd_unit(&db, &svc, max_history, &tx).await;
} else if svc.url.is_some() {
    uptime::probe_service(&db, &svc, timeout, max_history, &tx).await;
}
```

**`crates/vexboard-server/src/api/services.rs` (lines 175–188):** identical logic in the
immediate post-create background probe.

`probe_systemd_unit` (`crates/vexboard-server/src/probe/uptime.rs:195-206`,
`unit_active_state`) always connects to the **local** system D-Bus
(`zbus::Connection::system()`) and looks up a unit literally named `svc.systemd_unit`. It
has no concept of a remote host.

Separately, `crates/vexboard-frontend/src/components/discovery_panel.rs:98-99` sends
`systemd_unit: unit_name` unconditionally for every discovered unit, regardless of
`source` ("docker" | "podman" | "systemd"). For Docker/Podman discoveries — including
containers on a remote Docker host configured via `config.docker.sockets` (`tcp://...`)
— `unit_name` is the **container name**, not a systemd unit.

## Problem Definition

When a service is claimed from Docker/Podman discovery (local or remote host), the row
ends up with both `systemd_unit` = container name and `url` = the container's web UI
address. Because `systemd_unit.is_some()` is checked first, the probe dispatcher always
takes the systemd branch, queries the **local** D-Bus for a unit with that name, never
finds it, and records `"down"` — even though the service (local or remote) is fully
reachable via `url`, and the correct HTTP probe path is never invoked.

This must be fixed without reverting the 2026-06-07 `probe_priority_fix`, which is still
correct for services actually discovered via systemd (`discovery_source == "systemd"` or
manually configured with a real systemd unit name).

## Proposed Solution

Use `discovery_source` to disambiguate what `systemd_unit` actually contains. Only take
the systemd D-Bus branch when the service was NOT discovered via Docker/Podman:

```rust
let use_systemd = svc.systemd_unit.is_some()
    && !matches!(svc.discovery_source.as_deref(), Some("docker") | Some("podman"));

if use_systemd {
    probe_systemd_unit(...).await;
} else if svc.url.is_some() {
    probe_service(...).await;
}
```

Effects:
- `discovery_source == "systemd"` or `None` (manually configured, legacy rows) +
  `systemd_unit` set → systemd D-Bus probe, unchanged (preserves the *arr fix).
- `discovery_source == "docker"` or `"podman"` + `url` set → HTTP probe (fixed) —
  applies to both local and remote Docker/Podman hosts.
- No DB migration needed — existing misclassified rows self-heal on the next probe tick
  because the dispatcher re-reads `discovery_source` from the DB every cycle.

## Affected Files

1. `crates/vexboard-server/src/probe/mod.rs` — scheduled probe loop dispatcher
2. `crates/vexboard-server/src/api/services.rs` — immediate post-create probe dispatcher

## Implementation Steps

1. In `probe/mod.rs` lines 39–43: replace the `if svc.systemd_unit.is_some()` check with
   the `discovery_source`-aware condition above.
2. In `api/services.rs` lines 175–188: apply the identical change to the immediate
   post-create probe.

## Dependencies

No new dependencies. No Context7 lookup required — internal code change only, no
external library usage changed.

## Configuration Changes

None.

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`

All approved per CLAUDE.md. No FORBIDDEN COMMANDS used.

## Risks and Mitigations

- **Risk:** A service manually configured with `discovery_source` left unset but a
  genuine `systemd_unit` value would still be treated as systemd — correct, no change.
- **Risk:** A service with `discovery_source == "docker"`/`"podman"` but no `url` set
  gets no probe at all (same as today for services with neither field).
  **Mitigation:** Pre-existing behavior for any service missing both signals; out of
  scope for this fix.
- **Risk:** Does not stop the frontend from continuing to write the container name into
  `systemd_unit` for Docker/Podman claims.
  **Mitigation:** Out of scope — the dispatcher fix fully resolves the reported status
  bug without depending on frontend data hygiene, and avoids touching an unrelated
  frontend flow (Surgical Changes principle). The stored container name remains useful
  for the existing claim-conflict dedup check in `claim_service`.
