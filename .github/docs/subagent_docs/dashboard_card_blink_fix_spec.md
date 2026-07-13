# Dashboard Card Status Blink (Regression) — Spec

## Current State Analysis

- `DashboardPage` (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:170`) holds one
  flat `live_status: RwSignal<HashMap<i64, (String, Option<i64>)>>`, written on every
  SSE `probe` event (`mod.rs:181-194`) regardless of which service the event is for.
- `ServiceCard` (`crates/vexboard-frontend/src/components/service_card.rs:102-111`)
  builds its sparkline/history `LocalResource` with a source closure that reads
  `live_status.with(|m| m.get(&service_id).cloned())` purely to establish a reactive
  dependency (added in commit `94b1078`, "refetch sparkline history on live probe
  updates").
- Per Leptos's reactive graph (confirmed via Context7 `/leptos-rs/leptos` —
  `reactive_graph::traits::Track`, `subscriber.rs` `mark_dirty`/`mark_subscribers_check`):
  a signal read via `.with()`/`.get()` subscribes the *entire* current tracking scope
  to that *signal object*, not to the specific key read out of it. `RwSignal<HashMap<..>>`
  is a single signal, so **every** card's `history` resource closure subscribes to
  the *same* signal.
- Consequence: when any one service's probe result lands, `live_status.update(...)`
  marks **all** subscribers of that signal dirty — i.e. every rendered `ServiceCard`'s
  `history` `LocalResource` re-runs its source function and re-fetches
  `/api/v1/services/{id}/history`, concurrently, for every card on the dashboard, not
  just the one card whose status actually changed. This is what produces the
  "entire dashboard blinks" symptom the user is reporting: multiple simultaneous
  resource-loading transitions firing in lockstep across the whole grid on every SSE
  tick, rather than just the one changed card updating quietly.
- The 98.75%-scored review of `94b1078` explicitly flagged this exact mechanism as a
  known, accepted tradeoff ("history resources for all rendered cards will re-run on
  any card's SSE tick") but assessed it as merely "not a new class of over-fetching"
  rather than as a user-visible regression — the report from the user shows it is, in
  fact, the reintroduced blink.
- `current_status`/`current_latency` (`service_card.rs:115-124`) have the same coarse
  subscription, but they are synchronous derived `Signal::derive` values feeding plain
  text/class attributes — recomputing them doesn't cause a network round trip or a
  resource loading-state transition, so they don't visibly flash. Only the async
  `history` resource's re-run is visible as a blink.

## Problem Definition

Any SSE probe tick for *any* service causes *every* service card's sparkline/history
resource to refetch concurrently, producing a dashboard-wide visual blink instead of a
silent, isolated update of just the card whose status actually changed.

## Proposed Solution

Insert a per-card `Memo` between the raw `live_status` signal and the `history`
resource's source closure. Leptos `Memo`s recompute on every dirty notification from
their sources, but only notify *their own* subscribers when the newly computed value
is unequal (`PartialEq`) to the previous one. Since `(String, Option<i64>)` already
implements `PartialEq`, wrapping the per-service lookup in a `Memo` means:

- Every card's memo still recomputes on each SSE tick (cheap, synchronous, no network
  I/O) — no behavioral change there.
- Only the memo whose computed tuple actually changed (i.e. the one card whose
  `service_id` matches the incoming probe event) propagates a change notification
  downstream.
- Only that one `history` `LocalResource` re-runs and refetches. Unrelated cards' timers
  are untouched, eliminating the dashboard-wide blink while preserving the live
  sparkline refresh behavior added in `94b1078`.

This requires no change to `mod.rs` or the `live_status` data shape — surgical,
single-file change in `service_card.rs`.

## Implementation Steps

1. In `crates/vexboard-frontend/src/components/service_card.rs`, introduce a memoized
   per-service status/latency tuple and use it (instead of the raw `live_status.with()`
   read) as the `history` resource's reactive trigger:
   ```rust
   let live_entry = Memo::new(move |_| live_status.with(|m| m.get(&service_id).cloned()));

   let history = LocalResource::new(move || {
       live_entry.get();
       async move {
           if probe_enabled {
               fetch_history(service_id).await
           } else {
               Vec::new()
           }
       }
   });
   ```
2. No other files need changes.

## Dependencies

None — no new crates. Leptos `Memo` semantics verified via Context7
(`/leptos-rs/leptos`, reactive_graph subscriber/tracking docs) rather than assumed.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** `Memo::new` requires `T: PartialEq` (and `'static`). `(String, Option<i64>)`
  satisfies both, so no trait bound issues.
- **Risk:** Memo still recomputes for all cards on every tick (cost: one HashMap
  lookup + clone of a small tuple per card, synchronous). This is negligible compared
  to the eliminated network refetch and is unavoidable given the shared coarse signal;
  narrowing `live_status` itself into per-service signals would be a larger, riskier
  refactor touching `mod.rs` and is out of scope for this fix.
- **Out of scope:** `current_status`/`current_latency` remain coarse derived signals;
  they don't cause visible blinking (see analysis above), so left untouched per the
  surgical-change principle.

## Build/Test Commands (approved, from CLAUDE.md)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`

`trunk build`/`trunk serve` will not be run unless Trunk CLI + `wasm32-unknown-unknown`
presence is confirmed (FORBIDDEN COMMANDS gate); verification otherwise relies on
`cargo fmt`/`clippy`/code review of the reactivity change, consistent with the prior
`94b1078` spec's own caveat for this WASM-only crate.
