# Live Service-Status SSE Stream — Spec (FEAT-1)

Source: MASTER_PLAN.md HIGH PRIORITY / Features / FEAT-1 (F-F1, A-H4)

## Current State Analysis

- The backend already has everything needed except a handler that exposes it:
  - `AppState.probe_tx: broadcast::Sender<probe::uptime::ProbeEvent>`
    (`crates/vexboard-server/src/main.rs` — declared on the `AppState` struct and
    populated by `probe::start_probe_loop` in `main()`; the specific line numbers
    the master plan cites have shifted due to unrelated changes earlier in this
    session (SEC-1's `AppState` field addition) but the wiring itself is unchanged
    and requires no edit for this feature).
  - Every probe (`probe::uptime::probe_service` / `probe_systemd_unit`) already
    broadcasts a `ProbeEvent { service_id, service_name, url, status, latency_ms }`
    (`Debug, Clone, Serialize`) on completion.
  - The exact SSE pattern already exists and works: `api::metrics::metrics_stream`
    (`api/metrics.rs:37-51`) subscribes to `state.metrics_tx`, wraps it in
    `BroadcastStream`, maps to `Sse` `Event`s, and sets a 15s keep-alive.
  - No handler anywhere subscribes to `probe_tx` except the webhook notifier
    (`notify::notification_loop`) — nothing forwards `ProbeEvent`s to clients.
- Frontend currently has no live status mechanism. `pages/dashboard/service_grid.rs`
  renders `ServiceCard`s from a plain, non-reactive `ServiceData` struct built once
  per render from the `services: LocalResource<Vec<ServiceResponse>>` snapshot —
  status/latency only change when the whole resource is refetched.
- The "hard-coded sleep-then-refetch hack" cited in the master plan is in
  `pages/dashboard/modals.rs`'s `on_save` (service create flow):
  ```rust
  services.refetch();
  // The backend fires an immediate probe; wait briefly then refetch so
  // the status dot reflects the probe result rather than "unknown".
  TimeoutFuture::new(1_500).await;
  services.refetch();
  ```
  This only helps the just-created service, guesses a fixed 1.5s delay, and does
  nothing for every other service's ongoing status changes (a service going down
  mid-session never updates without a manual page reload).
- The existing client-side SSE consumption pattern to mirror is
  `components/metric_bar.rs`'s `MetricBar` component: `Effect::new` opens a
  `web_sys::EventSource`, wraps a `Closure` around a `set_signal` call, and
  registers it via `add_event_listener_with_callback` for a named event. That
  `Closure` is `.forget()`'d and never explicitly closed on unmount — a known,
  separately tracked issue (BUG-23, low priority, not in scope for this feature).
  This feature will follow the same pattern for consistency, carrying the same
  known (already-tracked) leak characteristic rather than fixing BUG-23 as a
  side effect here.

## Problem Definition

Service status/latency on the dashboard only ever reflects a point-in-time REST
fetch. There is no live update path, despite the backend already producing and
broadcasting exactly the events needed.

## Proposed Solution

### 1. Backend: `GET /api/v1/services/stream`

Add to `crates/vexboard-server/src/api/services.rs`, mirroring
`metrics_stream` exactly:
```rust
use std::convert::Infallible;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[utoipa::path(
    get,
    path = "/api/v1/services/stream",
    tag = "services",
    security(("cookieAuth" = [])),
    responses(
        (status = 200, description = "Server-sent event stream of ProbeEvent objects (text/event-stream)",
         content_type = "text/event-stream"),
        (status = 401, description = "Not authenticated"),
    )
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn stream_service_events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.probe_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event) => {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(Event::default().event("probe").data(data)))
        }
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```
Register in `read_router()` (viewer-protected, matching the master plan's
instruction): `.route("/stream", get(stream_service_events))`. No path conflict
with `/{id}` in `admin_router()` — matchit already prioritizes the existing static
`/reorder` route over the same `{id}` param route today, same principle applies.

Add `crate::api::services::stream_service_events` to the `paths(...)` list in
`api/openapi.rs`, next to `crate::api::metrics::metrics_stream`. `ProbeEvent` needs
no `ToSchema`/`components(schemas(...))` entry since the utoipa doc only declares
`content_type = "text/event-stream"` with no typed `body`, matching how
`metrics_stream` documents `SystemSnapshot` today.

### 2. Frontend: subscribe once, patch status/latency without a full refetch

In `pages/dashboard/service_grid.rs`:
- Add a small local struct mirroring the wire shape (mirrors `SystemMetrics` in
  `metric_bar.rs`):
  ```rust
  #[derive(Debug, Clone, serde::Deserialize)]
  struct ProbeEventFe {
      service_id: i64,
      status: String,
      latency_ms: Option<i64>,
  }
  ```
- Add `let live_status = RwSignal::new(std::collections::HashMap::<i64, (String, Option<i64>)>::new());`
  inside the `ServiceGrid` component body.
- Add an `Effect::new` (gated `#[cfg(target_arch = "wasm32")]`, matching
  `MetricBar`) that opens `EventSource::new("/api/v1/services/stream")`, listens
  for the `"probe"` event, parses `ProbeEventFe`, and does
  `live_status.update(|m| { m.insert(event.service_id, (event.status, event.latency_ms)); });`.
- In the render closure, capture `let overrides = live_status.get();` at the top
  of the existing `move || services.get().map(|svcs| { ... })` block (before
  `render_card` is defined), so the whole grid — already a full-rebuild-on-change
  design, not per-card fine-grained signals — reactively re-renders whenever any
  service's live status changes, same as it already does on `services.refetch()`.
- Inside `render_card`, when building `ServiceData`, prefer the live override over
  the snapshot value:
  ```rust
  let live = overrides.get(&svc.id);
  let status = live.map(|l| l.0.clone()).unwrap_or_else(|| svc.status.clone());
  let latency_ms = live.map(|l| l.1).unwrap_or(svc.latency_ms);
  ```
  and use `status`/`latency_ms` (not `svc.status`/`svc.latency_ms`) in the
  `ServiceData` literal.

This keeps the existing "rebuild the whole grid on any relevant signal change"
architecture (no new per-card reactive signal plumbing, no changes to
`ServiceCard`/`ServiceData`'s shape) while making every card's status/latency live.

### 3. Remove the now-redundant sleep-then-refetch hack

In `pages/dashboard/modals.rs`'s service-create `on_save`, remove the
`TimeoutFuture::new(1_500).await; services.refetch();` pair and its comment — the
just-created service's status now arrives via the SSE stream like any other
service's, once the backend's existing immediate post-create probe fires. The first
`services.refetch()` (to make the new row appear at all) stays; only the
timeout-then-refetch-again half is removed. If `TimeoutFuture`/`gloo_timers::future`
becomes unused elsewhere in the file, remove the now-unused import as an orphan of
this change.

## Implementation Steps

1. `crates/vexboard-server/src/api/services.rs` — add `stream_service_events`
   handler + route.
2. `crates/vexboard-server/src/api/openapi.rs` — register the new path.
3. `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` — add
   `ProbeEventFe`, `live_status` signal, SSE `Effect`, and override merge in
   `render_card`.
4. `crates/vexboard-frontend/src/pages/dashboard/modals.rs` — remove the
   sleep-then-refetch hack; drop the `gloo_timers::future::TimeoutFuture` import if
   it becomes unused as a result.

## Dependencies

None — `tokio-stream`, `axum`'s SSE support, and `web-sys`'s `EventSource` feature
are all already in use by the exact patterns being mirrored (`metrics_stream`,
`MetricBar`). `web-sys`'s `EventSource`/`MessageEvent` features are already enabled
(`Cargo.toml:15`, confirmed present).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** `EventSource` in `MetricBar` is never explicitly closed on unmount
  (BUG-23) — the new subscription in `ServiceGrid` will have the same
  characteristic. **Mitigation:** Accepted, consistent with existing code; BUG-23
  is a separately tracked, low-priority cleanup item covering all such
  `EventSource` usages together, not something to partially fix here.
- **Risk:** Re-rendering the whole grid on every probe event (rather than patching
  a single card in place) could feel heavier than necessary on a dashboard with
  many services. **Mitigation:** Matches the codebase's existing full-rebuild
  pattern (`services.refetch()` already rebuilds everything); for a self-hosted
  dashboard's realistic service counts this is an acceptable, low-risk tradeoff
  against introducing a materially larger per-card reactive-signal refactor.
- **Risk:** `live_status` grows unbounded if services are deleted while the map
  entry remains. **Mitigation:** Negligible for a small in-memory `HashMap` keyed
  by service ID in a browser tab that's reloaded periodically; not worth the added
  complexity of pruning logic for this feature.

## Files

- `crates/vexboard-server/src/api/services.rs` (new handler + route)
- `crates/vexboard-server/src/api/openapi.rs` (path registration)
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` (SSE subscription,
  live-status merge)
- `crates/vexboard-frontend/src/pages/dashboard/modals.rs` (remove sleep hack)
