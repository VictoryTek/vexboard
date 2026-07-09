# FEAT-3 — Uptime history endpoint + sparkline on service cards — Spec

## Current State Analysis

- `probe_results` (`001_init.sql:30-36`) stores up to `probe.max_history` (config, `src/config.rs:115`) rows per service: `status` (`up`/`down`/`unknown`), `latency_ms`, `checked_at`. Both probe functions (`src/probe/uptime.rs:109-122,171-186`) insert a row then trim to `max_history` via a `DELETE ... NOT IN (SELECT ... LIMIT ?)` pattern.
- The only current read of `probe_results` is `list_services` in `src/api/services.rs:79-90`, which joins the single latest row per service (`MAX(checked_at)` self-join) into `ServiceWithStatus`. No endpoint returns more than the latest row.
- `src/components/service_card.rs` renders `ServiceData` (id, display_name, status, latency_ms, etc. — no history) passed in as a prop from `src/pages/dashboard/service_grid.rs:132`, itself built from the `ServiceWithStatus` list fetched once via `list_services` and patched live via the `/api/v1/services/stream` SSE feed (FEAT-1, already shipped).
- No sparkline/chart component exists anywhere in the frontend. `metric_bar.rs` renders scalar values only (no history, no SVG data viz beyond static icons).
- Existing read-only per-resource pattern: `services::read_router()` currently has `/` and `/stream`; a per-ID sub-route (`/{id}/history`) is a natural addition placed alongside them.

## Problem Definition

Up to 100 historical probe results per service are collected and immediately thrown away after being used once for "latest status." Users have no way to see a service's recent latency trend or uptime percentage — only its current up/down state.

## Proposed Solution

1. **Backend** — add `GET /api/v1/services/{id}/history?limit=100` (viewer-protected, alongside `list_services`/`stream_service_events` in `read_router()`). Returns the most recent `limit` (default/max 100, clamped) `probe_results` rows for that service, oldest-first (so the frontend can render left-to-right chronologically without re-sorting), as a small DTO: `{ status, latency_ms, checked_at }[]`.
2. **Backend** — compute nothing server-side beyond the raw rows; uptime % and sparkline rendering are cheap client-side derivations from the same array (avoids a second endpoint/shape for "summary" vs "detail").
3. **Frontend** — in `service_card.rs`, add a `LocalResource` that fetches `/api/v1/services/{id}/history?limit=100` once per card mount. Render:
   - A latency sparkline: a small inline SVG `<polyline>` normalized to the card's fixed-height strip, using only rows where `latency_ms` is present.
   - An uptime-% strip: `count(status == "up") / count(*) * 100`, shown as compact text (e.g. `"98.5% uptime"`) near the sparkline, or omitted entirely if fewer than 2 data points exist (new/never-probed service).
   - Renders only when `probe_enabled` and history is non-empty — avoids clutter on services with no probe history yet.

## Implementation Steps

1. `src/db/models.rs`: add `ProbeHistoryPoint { status: String, latency_ms: Option<i64>, checked_at: Option<NaiveDateTime> }` (`Serialize`, `sqlx::FromRow`, `utoipa::ToSchema`).
2. `src/api/services.rs`:
   - Add `service_history` handler: `Path(id)`, `Query` for optional `limit` (default 100, clamp to `1..=100` so a client can't force an unbounded scan — matches the DB-side cap already enforced by `max_history` trimming).
   - Query: `SELECT status, latency_ms, checked_at FROM probe_results WHERE service_id = ? ORDER BY checked_at DESC LIMIT ?`, then `.into_iter().rev().collect()` in Rust to return oldest-first (simpler and more portable than a `SELECT * FROM (... DESC LIMIT ?) ORDER BY checked_at ASC` subquery, and the row count is capped at 100 so the extra reverse is free).
   - Register route: `.route("/{id}/history", get(service_history))` in `read_router()`.
   - Add `#[utoipa::path(...)]` doc block matching sibling handlers; register in `openapi.rs` paths + `ProbeHistoryPoint` in schemas.
3. `src/components/service_card.rs`:
   - Extend `ServiceData` with `probe_enabled: bool` (not currently passed through — needed to decide whether to bother fetching history at all) — check `service_grid.rs:132`'s call site to see whether `probe_enabled` is already available on the source data (`ServiceWithStatus.service.probe_enabled` — it is, via `Service`) and thread it through.
   - Add `async fn fetch_history(id: i64) -> Vec<HistoryPointFe>` helper (local struct mirroring the backend DTO).
   - Add `LocalResource::new(move || fetch_history(service_id))`, gated so it's only created/rendered when `probe_enabled` is true.
   - Render a small `<svg>` sparkline (viewBox-based, no external charting library — consistent with the "no new dependencies" and existing hand-rolled SVG icon style already used throughout this component) plus the uptime-% text, placed between the description row and the status-badge row.

## Dependencies

None new. Sparkline is a hand-rolled inline SVG `<polyline>`, consistent with existing hand-rolled SVG icons in `service_card.rs`/`metric_bar.rs`. No Context7 lookup required (Dependency Policy exemption: internal change, no new external library).

## Configuration Changes

None. Reuses existing `probe.max_history` as the practical upper bound; the endpoint's own `limit` query param is independently clamped to `100` as a defensive cap regardless of that config value.

## Risks & Mitigations

- **N+1 fetch on dashboards with many services**: each `ServiceCard` independently fetches its own history on mount. Acceptable for a dashboard-scale service count (tens, not thousands) and consistent with the existing one-resource-per-card mental model; a batch endpoint would be premature optimization not requested by the spec.
- **Empty/sparse history**: services that are new or have `probe_enabled = false` will have 0 history rows — sparkline/uptime-% must render nothing (not a broken/empty chart) in that case.
- **Query param validation**: an unclamped `limit` could be used to pull unbounded rows if a future caller passes a huge value — clamp server-side to `1..=100`.

## Files

- `crates/vexboard-server/src/db/models.rs` (new `ProbeHistoryPoint` DTO)
- `crates/vexboard-server/src/api/services.rs` (new handler + route)
- `crates/vexboard-server/src/api/openapi.rs` (register path + schema)
- `crates/vexboard-frontend/src/components/service_card.rs` (sparkline + uptime-% UI)
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` (thread `probe_enabled` into `ServiceData` if not already present)
