# Spec: Stop Service Cards Blinking on Probe SSE Updates

## Current State Analysis

- `ServiceGrid` ([crates/vexboard-frontend/src/pages/dashboard/service_grid.rs](../../../crates/vexboard-frontend/src/pages/dashboard/service_grid.rs))
  renders the dashboard's service card list. It has no `<For>` anywhere — all three sort-mode branches
  (Group: lines 201-319, Source: lines 333-461, default grid: lines 462-528) build cards via
  `.into_iter().map(render_card).collect_view()`, an **unkeyed** collection. Any reactive dependency read by
  the outer view closure invalidates the whole tree.
- `live_status` (line 44) is an `RwSignal<HashMap<i64, (String, Option<i64>)>>` populated from the
  `/api/v1/services/stream` SSE `probe` event handler (lines 52-69) — every individual probe result
  (any service, any time) inserts into this map.
- **Root cause** — line 89: `let overrides = live_status.get();` is called once, at the very top of the
  single `move || { ... }` closure passed as the `<Suspense>` children (line 88), which is the closure that
  builds the entire card list for whichever sort mode is active. Because this read happens at the top level,
  *any* update to `live_status` (i.e. *any* SSE probe event for *any* single service) reruns the entire
  closure — rebuilding all `EitherOf4` branches and every `ServiceCard` from scratch.
- Each `ServiceCard` creates its own `LocalResource` for probe history on mount
  ([service_card.rs:99](../../../crates/vexboard-frontend/src/components/service_card.rs#L99)). When a card
  is torn down and remounted, this resource is recreated, refetching history and causing the visible sparkline
  flash — compounded by every other card on the page also fully remounting for the same SSE tick, which is
  the "all cards blink" symptom.
- `ServiceData` ([service_card.rs:76-89](../../../crates/vexboard-frontend/src/components/service_card.rs#L76-L89))
  currently carries `status: String` and `latency_ms: Option<i64>` as plain owned values, computed once per
  card in `render_card` (service_grid.rs:102-104) by merging `overrides.get(&svc.id)` over the base
  `ServiceResponse` fields. This merge is what forces the read to happen at parent level today.
- `StatusDot` ([status_badge.rs](../../../crates/vexboard-frontend/src/components/status_badge.rs)) takes a
  plain `status: String` prop (not a signal) — cheap to recreate on a status change, not a concern.

## Problem Definition

The list-rebuilding pattern used for `services` (rebuild on `services.refetch()`, e.g. after add/delete/
reorder — acceptable, list membership actually changed) is also being used for `live_status`, which updates
far more frequently (every probe cycle, per service) and should only ever affect the one card whose status
changed. Because the reactive read is unscoped, every SSE tick forces a full rebuild of all cards, unmounting
and remounting `ServiceCard` (and its internal `LocalResource`), producing a visible blink across the entire
dashboard instead of a quiet, targeted update.

## Proposed Solution

Move the `live_status` read out of the parent's list-building closure and into each `ServiceCard`'s own
reactive scope, so a `live_status` change only patches the specific DOM text/class nodes that display status
and latency — it no longer participates in whether the outer card list closure reruns, so cards stay mounted
and their `LocalResource` for history is never recreated.

This is the minimal-blast-radius fix consistent with Leptos's fine-grained reactivity model: it does not
require introducing `<For>` with keyed diffing (which would be a much larger, riskier refactor touching all
three sort-mode branches and their drag-and-drop index-based logic), and does not change `services`/list-level
rebuild behavior, which is out of scope for this bug (list membership changes are rare and a full rebuild
there is acceptable/expected).

### Implementation Steps

1. **`crates/vexboard-frontend/src/pages/dashboard/service_grid.rs`**
   - Remove line 89 (`let overrides = live_status.get();`) from the top-level closure — the outer closure
     must no longer depend on `live_status` at all.
   - Remove line 102's `overrides.get(&svc.id)`-based merge; `render_card` should build `ServiceData` from
     the raw `svc.status` / `svc.latency_ms` (the last-fetched base values) only, unchanged otherwise.
   - Pass `live_status` (the `RwSignal<HashMap<i64, (String, Option<i64>)>>`, already `Copy`) down as a new
     prop on `ServiceCard` so each card can read its own entry reactively and independently.
2. **`crates/vexboard-frontend/src/components/service_card.rs`**
   - Add a `live_status: RwSignal<HashMap<i64, (String, Option<i64>)>>` prop to the `ServiceCard` component
     (import `std::collections::HashMap`).
   - Keep `service.status` / `service.latency_ms` as the base/fallback values (used before any SSE event
     arrives for this id, or if the map has no entry).
   - Replace the current static computation at lines 106/112 —
     ```rust
     let (badge_cls, status_label) = match service.status.as_str() { ... };
     let latency = service.latency_ms.map(|ms| format!("{ms}ms"));
     ```
     — with reactive closures that merge the live override over the base value each time they're read:
     ```rust
     let base_status = service.status.clone();
     let base_latency = service.latency_ms;
     let current_status = move || {
         live_status.with(|m| m.get(&service_id).map(|(s, _)| s.clone()))
             .unwrap_or_else(|| base_status.clone())
     };
     let current_latency = move || {
         live_status.with(|m| m.get(&service_id).and_then(|(_, l)| *l))
             .or(base_latency)
     };
     ```
   - Update the bottom status-row markup (around current lines 219-227) so the badge class, `StatusDot`,
     status label, and latency span are all recomputed inside `move ||` closures reading `current_status()` /
     `current_latency()`, instead of the static `badge_cls`/`status_label`/`latency` bindings computed once at
     component setup. This confines DOM churn to that one small row (a `<span>` class swap and text update)
     rather than the whole card.
   - No change needed to `history_strip()` / the `LocalResource` for probe history — it is unaffected once
     the card itself stops remounting.
3. No backend, schema, or API changes — this is purely a frontend reactivity-scoping fix.

## Dependencies

None. No new crates; `std::collections::HashMap` and `leptos::prelude::*` are already imported in the
relevant files. Context7 not required (internal frontend change, no new external library).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Passing `live_status` as a new required prop to `ServiceCard` changes its public component
  signature — any other call site must be updated.
  **Mitigation:** `ServiceCard` is only instantiated from `render_card` in `service_grid.rs` (confirmed single
  call site in the codebase); update is contained to that one file.
- **Risk:** Reading `live_status.with(...)` inside a per-card closure means *all* cards still technically
  subscribe to the same shared `HashMap` signal, so in theory every card's status closure reruns on every SSE
  tick, not just the affected card's.
  **Mitigation:** This is expected and acceptable — rerunning a small `move ||` closure that patches a
  `<span>` class/text is a cheap, invisible fine-grained DOM update in Leptos, not a remount. It does not
  recreate the `ServiceCard` component instance or its `LocalResource`, which is what caused the visible
  blink. This matches idiomatic Leptos usage (e.g. official examples merge shared signals into per-item
  display closures without introducing per-item derived signals for correctness).
- **Risk:** None to existing drag-and-drop, grouping, or sorting behavior — those code paths don't reference
  `live_status` or `overrides` and are untouched.
- **Risk:** None to backend — no files under `crates/vexboard-server/` are touched.

## Build/Test Commands Approved for This Spec (Phase 3 use)

Per FORBIDDEN COMMANDS and Resource Constraints in CLAUDE.md:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server` (frontend crate has no unit tests to run natively; WASM-only)
- `cargo build --release --bin vexboard-server` (does not compile the frontend; frontend correctness is
  verified via `cargo clippy --workspace`, which does compile `vexboard-frontend` for its WASM target per the
  existing workspace clippy config)
- `cargo audit --ignore RUSTSEC-2023-0071` (if installed)

No `trunk build`/`trunk serve` — not confirmed installed; not required since `cargo clippy --workspace`
already type-checks the frontend crate.
