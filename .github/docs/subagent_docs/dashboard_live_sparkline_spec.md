# Dashboard Live Sparkline/History Refresh — Spec

## Current State Analysis

- `DashboardPage` (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:120`) opens an
  `EventSource` against `/api/v1/services/stream` (`mod.rs:165-182`) and writes each
  incoming probe result into `live_status: RwSignal<HashMap<i64, (String, Option<i64>)>>`
  (`mod.rs:157`).
- `ServiceCard` (`crates/vexboard-frontend/src/components/service_card.rs:94`) derives
  `current_status`/`current_latency` from `live_status` via `Signal::derive`
  (`service_card.rs:112-121`), so the status badge and latency label update live and
  correctly — no bug there.
- The sparkline/uptime strip is fed by a *separate* one-shot fetch:
  `history: LocalResource<Vec<HistoryPointFe>>` (`service_card.rs:102-108`), created via
  `LocalResource::new(move || async move { fetch_history(service_id).await ... })`. Its
  source closure captures no reactive signal, so it runs exactly once when the card
  mounts and never again.
- Commit `5cde988` moved `live_status` up to `DashboardPage` specifically so
  `ServiceCard` no longer remounts on every SSE tick (fixing a visual "blink"). Before
  that fix, remounting accidentally re-ran `LocalResource::new` and thus refetched
  history on every tick. After the fix, nothing re-triggers the history fetch — this is
  an unintended side effect, not a designed behavior.
- Backend: `probe::uptime::probe_service`/`probe_systemd_unit`
  (`crates/vexboard-server/src/probe/uptime.rs`) insert into `probe_results` (line ~103
  and ~169) *before* broadcasting the `ProbeEvent` on `status_tx` (line ~135, ~202). So
  by the time the frontend's `live_status` map updates for a given `service_id`, the new
  history row is already committed and `GET /api/v1/services/{id}/history` will include
  it.

## Problem Definition

Sparklines (and the uptime-% label) on service cards only reflect data fetched at page
load; new probe results never appear until a full page reload, even though the status
badge/latency next to them updates live via SSE.

## Proposed Solution

Make the `history` `LocalResource`'s source closure reactive to the same per-service
`live_status` entry that already drives `current_status`/`current_latency`. Leptos
resources re-run their source function whenever a signal read inside it changes, so
reading `live_status.with(|m| m.get(&service_id).cloned())` inside the closure will
cause `history` to refetch each time a new probe result lands for that card — without
remounting the card (avoiding regression of the `5cde988` blink fix).

## Implementation Steps

1. In `crates/vexboard-frontend/src/components/service_card.rs`, change the `history`
   resource's source closure to read `live_status` for `service_id` before calling
   `fetch_history`, so it becomes a tracked dependency:
   ```rust
   let history = LocalResource::new(move || {
       let _trigger = live_status.with(|m| m.get(&service_id).cloned());
       async move {
           if probe_enabled {
               fetch_history(service_id).await
           } else {
               Vec::new()
           }
       }
   });
   ```
2. No other files need changes — `live_status` is already passed into `ServiceCard` as
   a prop (`service_card.rs:96`).

## Dependencies

None — no new crates, no Context7 lookup needed (internal Leptos reactivity pattern
already used elsewhere in this same file for `current_status`/`current_latency`).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Refetching on every probe tick could cause excess HTTP requests if a
  service has a very short `probe_interval`.
  **Mitigation:** This matches the existing tick cadence already used for the status
  badge (SSE events only fire on actual probe completion, gated server-side by each
  service's own `probe_interval`, minimum effectively ~5s scheduler tick). This is the
  same frequency previously used before `5cde988` (which triggered a full refetch via
  remount on every tick) — this change is strictly less frequent/costly than that prior
  behavior since it skips the full component remount.
- **Risk:** `probe_enabled = false` services have no `live_status` updates at all (they
  aren't probed), so their `history` resource, which returns `Vec::new()` immediately,
  simply won't be retriggered — correct, since there's nothing new to fetch.
- **Out of scope:** The `services`/`quick_links`/`groups` list resources
  (`mod.rs:130-133`) are one-shot and only refresh via explicit `.refetch()` after local
  CRUD actions; they don't pick up changes made in another browser tab or via discovery.
  This is a separate, pre-existing structural limitation not covered by this fix, since
  the user's stated complaint ("cards and card statuses and sparklines") is satisfied by
  the status-badge path already working live and this fix making sparklines live too.
  If the user still observes card list membership itself not updating without reload
  after this fix, that would need a separate spec (e.g. an SSE event on
  create/update/delete, or a polling refetch).

## Build/Test Commands (approved, from CLAUDE.md)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings` — will fail (workspace-wide native
  compile hits the WASM-only frontend crate per FORBIDDEN COMMANDS notes for
  `cargo build --workspace`/`cargo build`); use targeted alternative instead:
  scope clippy checks to backend only is not directly documented, but frontend crate is
  WASM-only, so `cargo clippy --workspace` is expected to behave like `cargo build
  --workspace` for the frontend target. **Correction:** per CLAUDE.md's own "Approved
  safe build/validation commands" list, `cargo clippy --workspace -- -D warnings` IS
  listed as approved (it compiles the server crate only on native target per the
  project's own documentation) — trust that documented behavior and run as listed.
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`

This is a single-file, single-line-of-reactivity change in a WASM-only crate, so full
verification of the fix itself requires `trunk build`/`trunk serve`, which is gated
under FORBIDDEN COMMANDS unless Trunk CLI + `wasm32-unknown-unknown` are confirmed
present. Phase 3 review should check for their presence before attempting a frontend
build; otherwise verification is limited to `cargo fmt --all -- --check` (safe on all
crates) and manual code review of the diff.
