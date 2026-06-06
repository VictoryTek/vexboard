# Service Grouping in UI — Specification
**Phase:** 1 — Research & Specification
**Date:** 2026-06-05
**Feature:** Feature Recommendation #6 from project_audit_2026-06-04

---

## 1. Current State Analysis

The `groups` feature is **fully implemented on the backend** but **entirely absent from the UI**:

| Layer | Status |
|---|---|
| `groups` DB table (id, name, icon, sort_order, created_at) | ✅ exists (001_init.sql) |
| `GET/POST/PUT/DELETE /api/v1/groups` API | ✅ implemented (api/groups.rs) |
| `group_id` column on `services` table | ✅ exists |
| `group_id` field on `CreateService` / `UpdateService` models | ✅ exists |
| `GET /api/v1/services` returns `group_id` in JSON | ✅ yes (via Service model) |
| Frontend `ServiceResponse` struct includes `group_id` | ❌ missing — field absent from struct |
| `EditFormData.group_id` wired to any UI control | ❌ `#[allow(dead_code)]` — hardcoded `None` |
| `EditModal` has a group selector | ❌ no UI control |
| `on_save` / `on_edit_save` send `group_id` to backend | ❌ field omitted from JSON body |
| Dashboard can render services grouped by group | ❌ no Group sort mode |

### Files to change:
- `crates/vexboard-frontend/src/components/modal_edit.rs` — modal lacks group selector
- `crates/vexboard-frontend/src/pages/dashboard.rs` — no group fetch, no Group sort mode, `group_id` dropped from serialization

### Files unchanged (backend is complete):
- All files under `crates/vexboard-server/` — no backend changes required

---

## 2. Problem Definition

1. Users cannot assign a service to a group via the UI — the only way is raw API calls
2. The dashboard has no "Group" sort mode — groups cannot be used to organize the service view
3. The group CRUD API is already exposed (Settings page) but services created/edited via UI always end up ungrouped

---

## 3. Proposed Solution Architecture

### 3.1 `modal_edit.rs` — Add group selector dropdown

New public struct `GroupItem { id: i64, name: String }` (used as a prop list).

Add `groups: Vec<GroupItem>` prop to `EditModal` (default = empty vec).
Add `(selected_group_id, set_selected_group_id): (ReadSignal<Option<i64>>, WriteSignal<Option<i64>>)` signal, initialized from `initial.group_id`.
Remove `#[allow(dead_code)]` from `EditFormData.group_id`.
Add `<select class="form-input">` after the Icon field, rendering:
- `<option value="">— No group —</option>` (selected when `selected_group_id == None`)
- One `<option value="{id}">` per group item
Wire `on:change` to parse the value and update `selected_group_id`.
Pass `group_id: selected_group_id.get()` in the `on_save` callback invocation.

### 3.2 `dashboard.rs` — Wire group data end-to-end

**Struct additions:**
- `GroupResponse { id: i64, name: String }` (used for fetching + rendering group sections)
- Add `group_id: Option<i64>` field to `ServiceResponse`

**New fetch function:**
- `async fn fetch_groups() -> Result<Vec<GroupResponse>, gloo_net::Error>` — `GET /api/v1/groups`

**New `LocalResource`:**
- `let groups = LocalResource::new(|| async move { fetch_groups().await.unwrap_or_default() })`

**Sort mode extension:**
- Add `SortMode::Group` variant
- Add "Group" button to the sort toggle strip

**`render_card` closure update:**
- Populate `group_id: svc.group_id` in the `EditFormData` constructed per card

**`on_save` / `on_edit_save` update:**
- Include `"group_id": data.group_id` in the `serde_json::json!({...})` body

**Group sort rendering:**
- Fetch the resolved groups list from the `LocalResource`
- For each group in sort_order order: collect services with matching `group_id`, render a section header (same pill + divider style as Source mode) followed by the card grid
- Any services with `group_id = None` or pointing to a deleted group are placed in an "Ungrouped" section (shown last, only if non-empty)

**Pass groups to EditModal:**
- Pass the resolved `Vec<GroupItem>` (mapped from `GroupResponse`) to each `EditModal` instance
- If the LocalResource is not yet resolved, pass an empty vec (graceful degradation — the selector shows only "No group")

### 3.3 Rendering contract for Group sort mode

```
┌─ [GroupName pill] ─────────────────────────────────┐
│  [card] [card] [card]                               │
└────────────────────────────────────────────────────┘
┌─ Ungrouped ─────────────────────────────────────────┐
│  [card] [card]                                      │
└────────────────────────────────────────────────────┘
```

Groups with zero visible services are omitted (same as Source mode).

---

## 4. Implementation Steps

1. **`modal_edit.rs`:**
   - Define `pub struct GroupItem { pub id: i64, pub name: String }`
   - Add `#[prop(default = vec![])] groups: Vec<GroupItem>` to `EditModal`
   - Remove `#[allow(dead_code)]` from `EditFormData.group_id`
   - Add `selected_group_id` signal (initialized from `initial.group_id`)
   - Add Group selector `<select>` (below Icon field, above Save buttons)
   - Wire `on:change` → `set_selected_group_id`
   - Set `group_id: selected_group_id.get()` in the save `EditFormData`

2. **`dashboard.rs`:**
   - Add `GroupResponse` struct with `id` and `name`
   - Add `group_id: Option<i64>` to `ServiceResponse`
   - Add `fetch_groups()` async fn
   - Add `let groups = LocalResource::new(...)` for groups
   - Add `SortMode::Group` enum variant
   - Add "Group" button to the sort toggle strip (after "Source")
   - Update `render_card` to capture `group_id: svc.group_id`
   - Update `on_save` to include `"group_id"` in new service POST body
   - Update `on_edit_save` to include `"group_id"` in PUT body
   - Implement `SortMode::Group` rendering branch in the `Suspense` block
   - Pass resolved groups list (as `Vec<GroupItem>`) to all `EditModal` instances

---

## 5. Dependencies

No new dependencies. Uses:
- `gloo_net` (already in frontend Cargo.toml) — for `fetch_groups()` HTTP call
- `leptos` (already present) — signals, `LocalResource`, `view!` macro
- `serde` / `serde_json` (already present) — JSON deserialization + serialization

Context7 verification: NOT required — no new external crates added.

---

## 6. Configuration Changes

None.

---

## 7. Build and Test Commands (Phase 3)

| Command | Purpose | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | Formatting | Zero compilation cost |
| `cargo clippy --workspace -- -D warnings` | Lint — checks server only on native target | Frontend WASM crate excluded from native clippy run |
| `cargo build --release --bin vexboard-server` | Backend binary still compiles | Verifies no server changes broke the build |

Frontend cannot be compiled for native target (WASM-only). Type correctness for Leptos components is verified indirectly by the `cargo clippy --workspace` pass (which the Leptos proc macros participate in at the workspace level) and by formatting checks.

---

## 8. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Groups `LocalResource` not yet resolved when modal opens | Pass empty `groups` vec — selector shows only "No group" (save still works; no group is assigned) |
| Group selector `<select>` `prop:value` not working in Leptos 0.8 | Use `on:change` with manual signal update + render `selected=true` on the matching `<option>` (standard Leptos pattern) |
| `SortMode::Group` breaks `Copy` derive on `SortMode` | `SortMode::Group` carries no data — `Copy` remains derivable |
| Services with deleted `group_id` reference fall through | Ungrouped section catches all `group_id = None` AND any `group_id` with no matching group in the fetched list |
| Frontend cannot be unit tested natively | Structural + type correctness verified by compilation; UI correctness requires Trunk dev server (out of scope for this workflow) |

---

## 9. File Inventory

Files to be modified:
- `crates/vexboard-frontend/src/components/modal_edit.rs`
- `crates/vexboard-frontend/src/pages/dashboard.rs`

Files to be created:
- `.github/docs/subagent_docs/service_grouping_spec.md` (this file)
- `.github/docs/subagent_docs/service_grouping_review.md` (Phase 3)
