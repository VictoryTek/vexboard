# FEAT-3 — Uptime history endpoint + sparkline on service cards — Review

## Spec Reference

`.github/docs/subagent_docs/FEAT-3_uptime_history_sparkline_spec.md`

## Changes Reviewed

- `crates/vexboard-server/src/db/models.rs` (new `ProbeHistoryPoint` DTO)
- `crates/vexboard-server/src/api/services.rs` (new `service_history` handler + route + `HistoryQuery`)
- `crates/vexboard-server/src/api/openapi.rs` (register path + schema)
- `crates/vexboard-frontend/src/components/service_card.rs` (sparkline + uptime-% UI, `probe_enabled` field)
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` (thread `probe_enabled` into `ServiceData`)

## 1. Specification Compliance

- `GET /api/v1/services/{id}/history?limit=100` added to `read_router()` (viewer-protected via the existing `viewer_protected` nest in `api/mod.rs` — no router-wiring change needed there, as predicted in the spec).
- `limit` clamped server-side to `1..=100`, mirroring the `AuditQuery` pattern in `audit.rs` exactly (default 100, `serde(default = "default_history_limit")`).
- Query returns `ORDER BY checked_at DESC LIMIT ?` then reverses in Rust to return oldest-first — matches the spec's stated approach and avoids a subquery.
- Frontend: `ServiceData` gained `probe_enabled`, threaded from the only construction site (`service_grid.rs:105`, verified via grep — no other call sites exist). `ServiceCard` fetches history only when `probe_enabled` is true (skips the fetch entirely otherwise, per spec).
- `history_strip` renders nothing when fewer than 2 points exist (new/unprobed services) — matches spec's "omitted entirely" requirement.
- Sparkline is a hand-rolled inline `<svg><polyline>`, no new dependency — matches spec.

## 2. Best Practices

- `HistoryQuery`/`default_history_limit` copy the exact `AuditQuery`/`default_limit` idiom already established in `audit.rs`, keeping pagination-param style consistent codebase-wide.
- Backend clamps `limit` even though the frontend always requests 100 — correct defensive practice since the endpoint is a general API surface, not frontend-only.
- Sparkline coordinate math (min/max normalize into a 0–20 viewBox height, `.max(1.0)` guard on `range` to avoid a divide-by-zero when all latencies are identical) is self-contained and has no floating-point panics.

## 3. Consistency

- Handler structure (`match sqlx::query_as::<_, T>(...).fetch_all(...).await { Ok/Err }`) matches every sibling handler in `services.rs`.
- Frontend fetch helper (`fetch_history`) matches the `fetch_discovered_units`/`fetch_groups_for_panel` shape in `discovery_panel.rs` (early-return-on-error via `let...else`, `.unwrap_or_default()`).
- `LocalResource::new(move || async move { ... })` matches the closure-with-capture form already used in `user_menu.rs`.

## 4. Maintainability

- `history_strip` is a small pure function (`Vec<HistoryPointFe> -> Option<impl IntoView>`) separated from the component body — testable in isolation if unit tests are ever added for frontend logic, and keeps the `ServiceCard` component body from growing unbounded.
- No abstraction beyond what's needed — no generic "chart" component, no shared history-fetching hook, since only one consumer exists (matches Simplicity First principle).

## 5. Completeness

- All spec implementation steps done: DTO, handler, route registration, OpenAPI registration, frontend fetch + render, `probe_enabled` threading.
- Uptime-% and sparkline both implemented (not just one of the two, as the MASTER_PLAN entry required both).

## 6. Performance

- Per-card independent fetch is a deliberate, spec-acknowledged tradeoff (N+1 at dashboard scale) — acceptable per spec's risk analysis for a self-hosted dashboard's typical service count.
- `probe_enabled` check on the frontend prevents unnecessary requests for services that will always return zero rows.
- Backend query is a single indexed-by-service_id, capped-at-100-rows SELECT — no N+1 on the server side.

## 7. Security

- Endpoint sits under `viewer_protected` (session-authenticated), same tier as `list_services`/`stream_service_events` — read-only history is appropriately viewer-accessible, not admin-only, consistent with the rest of the read surface.
- `limit` is parsed as `i64` and clamped before use in a parameterized `LIMIT ?` bind — no injection risk, no unbounded query risk.

## 8. API Currency

- No new external dependencies; uses only `axum::extract::Query`, `sqlx::query_as`, `utoipa::IntoParams` — all already pinned and used elsewhere in this codebase. Context7 lookup not required (Dependency Policy exemption).

## 9. Build Validation

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass — no diffs |
| `cargo clippy --workspace -- -D warnings` | Initially failed (`redundant_closure` on `history.get().and_then(\|points\| history_strip(points))`); fixed to `history.get().and_then(history_strip)`; re-check passes with 0 warnings (native-compiles the Leptos frontend crate cleanly) |
| `cargo test -p vexboard-server` | Pass — 34/34 tests, no SIGSEGV |
| `cargo build --release --bin vexboard-server` | Pass — clean release build |
| `cargo audit --ignore RUSTSEC-2023-0071` | Skipped — `cargo-audit` not installed on this machine |
| `trunk build` | Not run — Trunk CLI and `wasm32-unknown-unknown` target both confirmed absent; per FORBIDDEN COMMANDS this requires explicit approval before installing/running |

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 95% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 95% | A |
| Consistency | 100% | A |
| Build Success | 95% | A |

**Overall Grade: A (98.1%)**

(Functionality/Build Success held at 95% for the same reason as FEAT-2's review — no `trunk build`/browser verification of the actual rendered sparkline was possible in this environment. Performance held at 95% for the spec-acknowledged N+1 per-card fetch pattern, which is an accepted tradeoff rather than a defect.)

## Result

**PASS**
