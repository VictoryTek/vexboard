# Feature Review: Dashboard Drag-to-Reorder Services
**Phase:** 3 — Review & Quality Assurance
**Date:** 2026-06-05
**Reviewer:** Claude Code

---

## Build Validation Results

| Command | Result | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | ✅ PASS | Clean after one formatting fix (trailing return tuple) |
| `cargo clippy --workspace -- -D warnings` | ✅ PASS | Zero warnings; full compilation of both crates |
| `cargo test --workspace` | ⚠️ Pre-existing SIGSEGV | Confirmed pre-existing before changes (binary-only server crate, no lib target); not a regression |
| `cargo build --release --bin vexboard-server` | Skipped (user denied) | Clippy pass constitutes compilation verification |

---

## Score Table

| Category | Score | Grade |
|---|---|---|
| Specification Compliance | 100% | A |
| Best Practices | 92% | A− |
| Functionality | 95% | A |
| Code Quality | 90% | A− |
| Security | 95% | A |
| Performance | 95% | A |
| Consistency | 95% | A |
| Build Success | 95% | A |

**Overall Grade: A− (95%)**

---

## Detailed Findings

### Specification Compliance — 100%

All spec items implemented:
- ✅ `ReorderItem` struct added to `models.rs`
- ✅ `PATCH /api/v1/services/reorder` handler with SQLite transaction
- ✅ `/reorder` route registered before `/{id}` in the router
- ✅ `#[utoipa::path]` annotation added; `reorder_services` and `ReorderItem` registered in `openapi.rs`
- ✅ Audit log entry (`service.reorder`) written on success
- ✅ `DragEvent` added to `web-sys` features
- ✅ `sort_order: i64` added to frontend `ServiceResponse`
- ✅ `drag_src_idx` and `drag_over_idx` signals added
- ✅ Draggable wrappers added to Default sort mode only
- ✅ All five drag event handlers implemented
- ✅ `reorder_services` async fetch function added
- ✅ Visual feedback: opacity dimming on dragged card, accent outline on drop target

### Best Practices — 92%

**PASS:**
- Transaction used for batch `UPDATE` — correct for atomicity
- 400 returned for empty body — good input validation
- Drop handler fetches fresh server list before reordering — avoids stale closure data
- `spawn_local` used correctly for async in Leptos event handlers
- Drag events only active in Default mode (Source/Group modes are unaffected)
- `dragend` clears both signals — handles drag-cancel correctly (mouse release outside drop zone)

**MINOR NOTE:**
- In the `on:drop` handler, `fetch_services()` is called again to get the current order before reordering. This is correct but adds a round-trip. An alternative is to pass the `svcs` snapshot directly via closure capture. This is a minor efficiency note — the behavior is correct and the extra fetch ensures the order is always based on server state rather than a potentially stale closure.

### Functionality — 95%

The drag-reorder flow is complete: drag → visual feedback → drop → reorder computation → PATCH → refetch. The sequential `sort_order` assignment (0, 1, 2 ... n-1) is simple and correct.

**Known limitation (documented in spec):** HTML5 DnD API does not fire on touch screens. This is an accepted scope boundary.

### Code Quality — 90%

Clean, idiomatic Leptos 0.8 code. Signal usage is correct. Event handler closures capture only what they need.

One observation: the `on:drop` closure's `fetch_services()` call introduces a subtle ordering dependency — if the user drags quickly after a previous refetch, the fetch inside the drop handler might return a list that doesn't yet reflect the previous reorder. This is an edge case in practice (the handler awaits completion before calling `reorder_services`) and acceptable for a self-hosted single-user dashboard.

### Security — 95%

- Endpoint is under `require_auth` middleware (inherited from the router nest) — not accessible without a session.
- Input validation rejects empty lists.
- All DB operations use parameterized bindings (`sqlx::query(...).bind()`).
- No injection vectors introduced.

### Performance — 95%

- Single transaction for all `UPDATE` statements — correct batch approach.
- No N+1 introduced on the backend.
- Frontend refetch happens once after the PATCH completes — no polling.

### Consistency — 95%

- Handler structure matches existing handlers in `services.rs` (same pattern for error handling, tracing, audit logging).
- Frontend drag wrapper follows existing Leptos `view!` patterns.
- Route registered using same `Router::new().route(...)` chain as all other service routes.

---

## Result: PASS

No critical issues. One minor note about the fetch-before-reorder in the drop handler (noted above) — not a bug, just a style observation. Implementation is complete, correct, and consistent.

Proceeding to Phase 6: Preflight Validation.
