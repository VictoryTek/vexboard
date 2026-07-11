# BUG-9 — Default-View Drag Reorder Manipulates Wrong Item on `sort_order` Ties — Spec

## Current State Analysis

`ServiceGrid` (`crates/vexboard-frontend/src/pages/dashboard/service_grid.rs`) has four render
branches (`EitherOf4::A/B/C/D` for group/source/... views). The default, unsectioned view
(`EitherOf4::D`, lines 458-524) renders cards from:
```rust
let mut svcs = svcs;
svcs.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
    .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
```
(lines 460-461) — sorted by `sort_order` with a display-name tiebreak — and assigns `idx` to
each card from that sorted order via `.enumerate()` (line 462-463), which drives `drag_src_idx`
/ `drag_over_idx` (lines 477, 480).

Its `on:drop` handler (lines 487-509) then does:
```rust
if let Ok(mut current) = fetch_services().await {
    let item = current.remove(src_i);
    current.insert(dst_i, item);
    ...
}
```
`fetch_services()` returns services in the backend's raw fetch order — `list_services`
(`crates/vexboard-server/src/api/services.rs:77`) queries `ORDER BY sort_order ASC` with **no**
secondary sort key, so among services sharing the same `sort_order` (e.g. multiple services
created via discovery-claim before any manual reorder ever ran, all defaulting to `sort_order =
0`), the DB's tie order is arbitrary/insertion-order-dependent — not guaranteed to match the
display's alphabetical tiebreak. `src_i`/`dst_i` were computed against the *display*-sorted
order; applying them via `.remove()`/`.insert()` to `current` (the *raw-fetch*-order list)
silently moves whatever item happens to sit at that raw index — the wrong item whenever a
`sort_order` tie exists and the DB's tie order differs from the display's.

The two sectioned views (`EitherOf4::B` — grouped, lines 237-266; `EitherOf4::C` — by source,
confirmed via the analogous code past line 379) **already** apply the identical `sort_by`
tiebreak to the freshly-fetched list before computing drop indices (e.g. lines 252-253:
`section.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then_with(...))`), so they are correct.
Only the unsectioned default view's `on:drop` handler is missing this step.

## Problem Definition

Dragging a card to reorder it in the default (unsectioned) view can silently reorder a
*different* service than the one the user dragged, whenever two or more services share the same
`sort_order` and the backend's tie order differs from the frontend's display order.

## Proposed Solution

Apply the exact same `sort_by(sort_order, then display_name.to_lowercase())` tiebreak to
`current` immediately after fetching, before computing `remove`/`insert` indices — mirroring
the pattern already used correctly in the two sectioned views.

## Implementation Steps

In `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs`, inside the default view's
`on:drop` handler (around line 495-506), change:
```rust
spawn_local(async move {
    if let Ok(mut current) = fetch_services().await {
        let item = current.remove(src_i);
        current.insert(dst_i, item);
        let payload: Vec<_> = current.iter()
            .enumerate()
            .map(|(i, s)| (s.id, i as i64))
            .collect();
        let _ = reorder_services(payload).await;
        services.refetch();
    }
});
```
to:
```rust
spawn_local(async move {
    if let Ok(mut current) = fetch_services().await {
        current.sort_by(|a, b| a.sort_order.cmp(&b.sort_order)
            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
        let item = current.remove(src_i);
        current.insert(dst_i, item);
        let payload: Vec<_> = current.iter()
            .enumerate()
            .map(|(i, s)| (s.id, i as i64))
            .collect();
        let _ = reorder_services(payload).await;
        services.refetch();
    }
});
```

## Dependencies

None.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** None — this brings the default view's drop handler in line with the already-correct
  behavior of the grouped and by-source views, using an identical, already-proven sort
  expression copied verbatim from those handlers (and from this same view's own render sort at
  lines 460-461).

## Test Plan

This is a `wasm32`-only frontend component (Leptos CSR); the project's `cargo test -p
vexboard-server` scope does not build or test frontend code, and per CLAUDE.md the frontend
crate is WASM-only and must never be built/tested for the native target. No automated test
exists or is added for this fix — verified by inspection: the corrected block is a byte-for-byte
copy of the tiebreak expression already used (and already correct) in `EitherOf4::B`'s drop
handler (lines 252-253) and in this same view's own render-sort (lines 460-461), so its
correctness follows directly from that existing, working precedent. `cargo fmt`/`clippy`/tests
for the backend crate are unaffected since no backend file changes.
