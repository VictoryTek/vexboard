# Feature Spec: Dashboard Drag-to-Reorder Services
**Phase:** 1 — Research & Specification
**Date:** 2026-06-05
**Scope:** `crates/vexboard-server` (backend) + `crates/vexboard-frontend` (frontend WASM)

---

## 1. Current State Analysis

### Schema
Both `services` and `groups` tables already have `sort_order INTEGER NOT NULL DEFAULT 0` (`001_init.sql`). `quick_links` has it too.

### Backend
- `GET /api/v1/services` already fetches `ORDER BY sort_order ASC` (`services.rs:47`).
- `GET /api/v1/groups` already fetches `ORDER BY sort_order ASC` (`groups.rs:34`).
- `sort_order` is writable via `PUT /api/v1/services/{id}` and `PUT /api/v1/groups/{id}` (individual updates only — no batch reorder endpoint exists).
- No `PATCH /reorder` endpoint exists for either resource.

### Frontend
- `ServiceResponse` struct in `dashboard.rs:19–32` does not include `sort_order`.
- Dashboard renders services in three modes (`Default`, `Source`, `Group`). Default mode sorts by `sort_order` but there is no drag-and-drop interaction.
- No drag state exists in the component.
- `web-sys` dependency has limited features enabled (`EventSource`, `MessageEvent`, `HtmlInputElement`, `HtmlElement`, `CssStyleDeclaration`, `Window`, `Storage`, `Location`). `DragEvent` is absent.

---

## 2. Problem Definition

`sort_order` columns exist and services are already displayed in `sort_order` order, but there is no UI mechanism to change the order. Users are unable to arrange their dashboard layout.

---

## 3. Proposed Solution Architecture

### 3.1 Backend

Add a single batch-reorder endpoint for services:

```
PATCH /api/v1/services/reorder
Body: [{"id": 3, "sort_order": 0}, {"id": 1, "sort_order": 1}, ...]
```

- Runs all `UPDATE` statements in a single SQLite transaction for atomicity.
- Validates that the request body is non-empty.
- Returns `200 OK` on success; `400 Bad Request` if body is empty.
- Protected by `require_auth` middleware (already applied to the services nest).
- Writes a single `service.reorder` audit event with a JSON summary of the new order.
- Full `utoipa` annotation for OpenAPI spec.

**No group or quick-link reorder endpoints** — the audit entry specifies services only; groups and quick-links can be addressed in a future iteration if requested.

### 3.2 Frontend

**New web-sys feature:** `DragEvent` — required for typed drag event handlers in Leptos 0.8 CSR (`on:dragstart`, `on:dragover`, `on:drop`, `on:dragend`).

**`ServiceResponse` struct:** Add `sort_order: i64` field so the frontend knows the current order and can compute new values after a drop.

**Drag state signals** (added to `DashboardPage`):
```rust
let drag_src_idx: RwSignal<Option<usize>> = RwSignal::new(None);
let drag_over_idx: RwSignal<Option<usize>> = RwSignal::new(None);
```

**Drag target:** Each service card in **Default sort mode only** is wrapped in a `<div draggable="true">`. Source and Group modes sort by computed keys (source type, group name) — drag-reorder does not apply there. The draggable wrapper is not added in those branches.

**Interaction model:**
1. `dragstart` → `drag_src_idx.set(Some(idx))` + set drag cursor CSS
2. `dragover` → `ev.prevent_default()` (required by browser to allow drop) + `drag_over_idx.set(Some(idx))`
3. `dragleave` → clear `drag_over_idx` for that slot
4. `drop` → `ev.prevent_default()`, read `drag_src_idx` + `drag_over_idx`, reorder the local vec, assign new sequential `sort_order` values (0, 1, 2 … n-1), PATCH the backend, clear drag state, `services.refetch()`
5. `dragend` → clear both signals (handles drag-cancel)

**Visual feedback:**
- Dragged card: `opacity: 0.5`
- Drop target card: `outline: 2px solid var(--color-accent); border-radius: 12px;`
- Drag cursor on wrapper: `cursor: grab`

**Sort-order assignment on drop:** After moving item from index `src` to index `dst`, reassign `sort_order` as the item's new position index (0-based). This is simple, deterministic, and matches how the DB query already sorts.

**PATCH call:**
```json
PATCH /api/v1/services/reorder
[{"id": 3, "sort_order": 0}, {"id": 1, "sort_order": 1}, ...]
```
Uses `gloo_net::http::Request::patch`. On error (non-2xx), calls `services.refetch()` to restore server-side order (optimistic update is not used — we just fire-and-refetch).

---

## 4. Implementation Steps

### Step 1 — `crates/vexboard-server/src/db/models.rs`
Add:
```rust
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReorderItem {
    pub id: i64,
    pub sort_order: i64,
}
```

### Step 2 — `crates/vexboard-server/src/api/services.rs`
1. Add `patch` to the `axum::routing` imports.
2. Register `.route("/reorder", patch(reorder_services))` in `router()`.
   - **IMPORTANT:** the `/reorder` literal route must be registered **before** `/{id}` or it will be shadowed. Since `/{id}` only accepts `put` and `delete`, and `/reorder` accepts `patch`, Axum routes by method + path — there is no actual conflict. But to be explicit and safe, register `/reorder` first.
3. Implement `reorder_services` handler:
   - Accept `Json(Vec<ReorderItem>)`.
   - Return `400` if vec is empty.
   - Open a transaction via `state.db.begin()`.
   - Loop: `UPDATE services SET sort_order = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?` for each item.
   - Commit transaction.
   - Write `service.reorder` audit entry with detail JSON `{"count": n}`.
   - Return `200 OK {"status": "reordered"}`.
4. Add full `#[utoipa::path]` annotation.

### Step 3 — `crates/vexboard-frontend/Cargo.toml`
Add `DragEvent` to the `web-sys` features list.

### Step 4 — `crates/vexboard-frontend/src/pages/dashboard.rs`
1. Add `sort_order: i64` to `ServiceResponse`.
2. Add `drag_src_idx` and `drag_over_idx` signals.
3. In the Default sort branch (`EitherOf4::D`), replace the bare `render_card(svc)` call with a `<div>` wrapper carrying `draggable="true"` and the five drag event handlers.
4. Add `reorder_services` async function at the bottom of the file:
   ```rust
   async fn reorder_services(items: Vec<(i64, i64)>) -> Result<(), gloo_net::Error> {
       let body: Vec<_> = items.iter().map(|(id, so)| serde_json::json!({"id": id, "sort_order": so})).collect();
       let req = gloo_net::http::Request::patch("/api/v1/services/reorder").json(&body)?;
       req.send().await?;
       Ok(())
   }
   ```

---

## 5. Dependencies

No new Cargo dependencies. `DragEvent` is a feature flag of the already-present `web-sys = "0.3"` crate.

| Library | Usage | Already present? |
|---|---|---|
| `web-sys` | `DragEvent` feature for drag event types | Yes — adding feature only |
| `gloo-net` | `Request::patch` for PATCH call | Yes |
| `serde_json` | Serialize reorder payload | Yes |

---

## 6. Configuration Changes

None. No new config fields required.

---

## 7. Build/Test Commands for Phase 3

Per CLAUDE.md approved commands:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`

Resource cost: all are within documented constraints. `cargo build --release --bin vexboard-server` compiles only the native server binary. Frontend compilation is not validated locally (requires Trunk + wasm32 target; Phase 3 validates only backend compilation and clippy).

---

## 8. Files to be Modified

| File | Change |
|---|---|
| `crates/vexboard-server/src/db/models.rs` | Add `ReorderItem` struct |
| `crates/vexboard-server/src/api/services.rs` | Add `reorder_services` handler + route |
| `crates/vexboard-frontend/Cargo.toml` | Add `DragEvent` to web-sys features |
| `crates/vexboard-frontend/src/pages/dashboard.rs` | Add `sort_order` to `ServiceResponse`, drag signals, drag wrappers in Default mode, `reorder_services` fetch fn |

---

## 9. Risks and Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Route `/reorder` shadowed by `/{id}` | Low | Routes use different HTTP methods (`PATCH` vs `PUT`/`DELETE`); no conflict |
| Concurrent reorder requests clobbering each other | Low | SQLite WAL mode serializes writes; last writer wins, which is acceptable for single-user dashboard |
| Frontend drag state leaks across sort mode changes | Low | `drag_src_idx` and `drag_over_idx` are only read/written inside the Default branch; signals remain but have no visual effect in other modes |
| `DragEvent` feature expanding binary size | Negligible | web-sys feature adds one small JS binding struct |
| Drag-and-drop not working on mobile / touch screens | Medium | HTML5 DnD API does not fire on touch devices; this is a known limitation of the API. Touch-based reorder (touch events) is out of scope for this implementation; a future iteration can address it |
