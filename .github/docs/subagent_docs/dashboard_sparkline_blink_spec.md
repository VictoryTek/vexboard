# Dashboard Sparkline Blink / Scroll-Jump Fix — Specification

## Current State Analysis

Real-time status arrives via SSE (`/api/v1/services/stream`). In
`crates/vexboard-frontend/src/pages/dashboard/mod.rs`, a single
`live_status: RwSignal<HashMap<i64, (String, Option<i64>)>>` is updated per
`probe` event.

In `crates/vexboard-frontend/src/components/service_card.rs`:

- `live_entry = Memo::new(|_| live_status.with(|m| m.get(&service_id).cloned()))`
  — per-service memo (added in commit 5513366 to stop *all* cards reacting to
  one tick).
- `history = LocalResource::new(move || { live_entry.get(); async { fetch_history(...) } })`
  — refetches sparkline history whenever this card's probe entry changes
  (added in commit 94b1078).
- View line 226: `{move || history.get().and_then(history_strip)}` — renders the
  sparkline strip only when `history.get()` is `Some`.

## Problem Definition

A probe cycle probes **every** service, so the server emits a `probe` SSE event
for each card within the same tick. Each event changes that card's `live_entry`
memo, which re-runs its `history` `LocalResource`. While a `LocalResource` is
refetching, `history.get()` returns `None` (Leptos resources pass through a
pending/`None` state on every reload). Line 226 therefore renders nothing, so
**every card's sparkline strip collapses at the same moment**. All cards shrink,
total page height drops, and the browser clamps the scroll position toward the
top; when the fetches resolve the strips reappear. This is the observed
"blinking" and the forced scroll-to-top.

The two prior fixes changed *which* resources refetch, but not the fact that a
refetching resource momentarily yields `None` and blanks the strip.

## Proposed Solution

Retain the last successfully-resolved history for each card so the sparkline
never blanks during an in-flight refetch:

- Add `held_history: RwSignal<Vec<HistoryPointFe>>` initialised empty.
- Add an `Effect` that copies `history.get()` into `held_history` only when it
  is `Some` (skip while `None`/loading, preserving the prior strip).
- Render from `held_history` instead of the resource:
  `{move || history_strip(held_history.get())}`.

`history_strip` already returns `None` for `< 2` points, so the very first load
(empty vec) shows no strip until data arrives — unchanged behaviour. Subsequent
probe ticks keep the previous strip visible until fresh data replaces it, so
card height stays constant, nothing collapses, and scroll position is preserved.

Storing an async result in a signal is the Leptos-endorsed pattern; the `Effect`
only *reads* the resource and *writes* a distinct signal it never reads, so no
update loop is created.

## Implementation Steps

1. In `service_card.rs`, after the existing `history` `LocalResource`, add
   `held_history` signal + guarding `Effect`.
2. Change view line 226 from
   `{move || history.get().and_then(history_strip)}` to
   `{move || history_strip(held_history.get())}`.

## Dependencies

None. No new crates or APIs. `HistoryPointFe` already derives `Clone`.

## Configuration Changes

None.

## Risks & Mitigations

- **Stale strip on a service that stops probing:** the strip holds its last
  window rather than clearing. Acceptable — the status badge (driven separately
  by `current_status`) still reflects live state, and history is a rolling view.
- **Effect writing to a signal:** limited to persisting an async result, which
  is the recommended Leptos idiom; the written signal is never read by the
  effect, so there is no reactive loop.
