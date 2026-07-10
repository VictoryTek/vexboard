# Spec: Record D-Bus Latency for systemd-Probed Services

## Current State Analysis

- Service cards render a latency sparkline + uptime-% strip (`history_strip()` in
  [crates/vexboard-frontend/src/components/service_card.rs](../../../crates/vexboard-frontend/src/components/service_card.rs#L27-L58)).
  - The uptime-% text is derived from `status` (up/down ratio) — works for every service type today.
  - The **visual polyline** is built only from non-null `latency_ms` values (line 36-58). It requires
    `latencies.len() >= 2` to draw a line at all.
- Two probe strategies write to `probe_results (service_id, status, latency_ms, checked_at)`:
  - `probe_service()` (URL/HTTP probes) in
    [crates/vexboard-server/src/probe/uptime.rs](../../../crates/vexboard-server/src/probe/uptime.rs#L46-L144) —
    measures wall-clock elapsed time via `Instant::now()` / `start.elapsed()` around the HEAD/GET request,
    and stores it as `latency_ms`.
  - `probe_systemd_unit()` (systemd/Docker/Podman-via-systemd probes), same file, lines 148-207 — queries
    D-Bus (`ListUnits` via `SystemdManagerProxy`) for the unit's `active_state` and **always inserts
    `latency_ms = NULL`** (line 174: `.bind(None::<i64>)`), and always broadcasts `latency_ms: None` in the
    `ProbeEvent` (line 202).
- Dispatch logic in
  [crates/vexboard-server/src/probe/mod.rs](../../../crates/vexboard-server/src/probe/mod.rs#L38-L53) routes
  a service to `probe_systemd_unit` whenever it has a `systemd_unit` and wasn't discovered via
  `docker`/`podman` discovery_source; this covers plain systemd services as well as
  systemd-managed Docker/Podman container units (e.g. `docker-joplin-server.service`) observed in production
  data on the user's server.
- Net effect confirmed against a live instance: systemd-probed services (Joplin Server/Db, Nginx Proxy
  Manager, Seerr — all `discovery_source: "systemd"`) accumulate `probe_results` rows with `status` set but
  `latency_ms` always `NULL`, so `history_strip()` shows the uptime-% text but never draws the sparkline
  line. URL-probed services (Mealie, Sonarr) get both.
- Schema already supports this: `probe_results.latency_ms` is a nullable `INTEGER`
  ([001_init.sql:34](../../../crates/vexboard-server/src/db/migrations/001_init.sql#L34)) — **no migration
  needed**.
- The `/api/v1/services/{id}/history` endpoint
  ([crates/vexboard-server/src/api/services.rs:170-189](../../../crates/vexboard-server/src/api/services.rs#L170-L189))
  is already generic (`SELECT status, latency_ms, checked_at ... WHERE service_id = ?`) — **no backend API
  change needed**.
- Frontend `history_strip()` already handles `latency_ms: Option<i64>` generically and draws the polyline
  whenever ≥2 non-null latency values exist — **no frontend change needed** once the backend supplies real
  values.

## Problem Definition

`probe_systemd_unit()` never measures how long the D-Bus round trip took, so systemd/Docker/Podman-discovered
services can never populate enough non-null `latency_ms` points to draw the sparkline line, even though they
accumulate uptime history correctly. This produces an inconsistent card UI: some services show a full
sparkline + uptime %, others show only the uptime % text, for reasons not visible to the user and unrelated
to actual service type in any user-facing sense.

## Proposed Solution

Measure the elapsed wall-clock time of the D-Bus `ListUnits` call (the same call `unit_active_state()`
already performs) in `probe_systemd_unit()`, using the same `Instant`/`elapsed().as_millis()` pattern already
used in `probe_service()`, and store/broadcast it as `latency_ms` instead of a hardcoded `None`.

This keeps the sparkline's meaning consistent across all service types (a latency trend line) with a
minimal, localized change — no schema, API, or frontend changes required.

### Implementation Steps

1. In `crates/vexboard-server/src/probe/uptime.rs`, inside `probe_systemd_unit()`:
   - Start an `Instant::now()` immediately before the `unit_active_state(&unit_name).await` call (mirroring
     the `let start = Instant::now();` pattern in `probe_service()` at line 64).
   - Compute `elapsed_ms = start.elapsed().as_millis() as i64` after the call resolves — on both the `Ok`
     and `Err` branches of `unit_active_state`, so a failed/slow D-Bus query still records a latency instead
     of silently going to `None` (a failed lookup is itself meaningful latency/timeout information, matching
     how `probe_service()` records latency for GET fallback failures where available).
   - Replace `.bind(None::<i64>)` at line 174 with `.bind(Some(elapsed_ms))` (or `.bind(elapsed_ms)` if using
     a plain `i64`, matching whichever the existing insert / `Option<i64>` typing prefers — `probe_service`
     binds an `Option<i64>` at line 112, so keep the same `Option<i64>` shape for consistency).
   - Update the `ProbeEvent` construction at line 197-203 to set `latency_ms: Some(elapsed_ms)` instead of
     `latency_ms: None`, so live SSE-pushed status updates (if consumed anywhere for latency display) are
     consistent with what gets persisted.
2. No changes needed to:
   - `crates/vexboard-server/src/db/migrations/` (column already nullable `INTEGER`).
   - `crates/vexboard-server/src/api/services.rs` (`service_history` handler is already generic).
   - `crates/vexboard-frontend/src/components/service_card.rs` (`history_strip()` already draws the polyline
     whenever ≥2 non-null latencies are present, with no service-type conditional).
3. Manual verification against the user's live instance (informational, not part of automated
   test/preflight): after deploying, confirm `probe_results.latency_ms` becomes non-null for
   systemd-probed services (Joplin Server/Db, Nginx Proxy Manager, Seerr) after at least 2 probe cycles, and
   that the sparkline line renders on those cards.

## Dependencies

None. No new crates. `std::time::Instant` / `Duration` are already imported at the top of
`crates/vexboard-server/src/probe/uptime.rs` (line 1) and used by `probe_service()`. Context7 lookup was not
required per CLAUDE.md policy (internal code change, no new external library).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** D-Bus `ListUnits` latency is not a meaningful proxy for "how responsive is this service" the way
  an HTTP round trip is — it measures systemd/D-Bus responsiveness, not the underlying service.
  **Mitigation:** This is an accepted tradeoff per the chosen approach (user selected "measure D-Bus latency"
  over changing the sparkline to plot status). It still gives systemd-probed cards a real, non-degenerate
  latency trend line consistent with the rest of the UI, and D-Bus latency spikes can themselves be a useful
  signal (e.g. system under load).
- **Risk:** Recording latency on the `Err` branch of `unit_active_state` could store timeout-length latencies
  that skew the min/max normalization in the sparkline (line 42-44 of `service_card.rs`) when a unit
  repeatedly fails to resolve.
  **Mitigation:** This mirrors existing behavior in `probe_service()`, which already records latency on GET
  failures where a response was received; no new failure-handling pattern is introduced. No mitigation beyond
  existing behavior is in scope for this change.
- **Risk:** None to backward compatibility — `latency_ms` was always nullable and consumers (frontend,
  history endpoint) already treat it as `Option<i64>`.

## Build/Test Commands Approved for This Spec (Phase 3 use)

Per FORBIDDEN COMMANDS and Resource Constraints in CLAUDE.md, only these are in scope:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
- `cargo audit --ignore RUSTSEC-2023-0071` (if installed)

No `trunk build`/`trunk serve` needed since no frontend files change.
