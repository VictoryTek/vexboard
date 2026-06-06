# Groups Management UI — Specification

**Feature:** Group Management Panel + Discovery Panel Group Assignment  
**Date:** 2026-06-05  
**Phase:** 1 — Research & Specification

---

## 1. Current State Analysis

### Backend (complete)
- `GET/POST /api/v1/groups` — list and create groups
- `PUT /api/v1/groups/{id}` — rename, change icon, change sort_order
- `DELETE /api/v1/groups/{id}` — delete group (services retain their group_id, fall to "Ungrouped")
- `Group` model: `{ id, name, icon: Option<String>, sort_order: i64, created_at }`
- `CreateGroup`: `{ name, icon?, sort_order? }`
- `UpdateGroup`: `{ name?, icon?, sort_order? }`

### Frontend (partial)
- `dashboard.rs`: fetches groups via `LocalResource`, renders Group sort mode, passes groups to `EditModal`
- `modal_edit.rs`: has group dropdown (`<select>`) — hidden when groups list is empty
- `discovery_panel.rs`: opens `EditModal` but **never passes groups**, so group dropdown never appears for discovered services; also **omits `group_id`** from the POST body entirely
- No UI exists to create, rename, reorder, or delete groups

---

## 2. Problem Definition

1. Groups cannot be created from the UI — the backend API exists but nothing calls it
2. Discovered services cannot be assigned to a group when added from the discovery panel
3. The "Group" sort mode button is present but useless until problems 1 and 2 are solved

---

## 3. Proposed Solution Architecture

### 3a. New component: `modal_groups.rs`

A modal dialog for full group lifecycle management:

```
┌────────────────────────────────────────┐
│  Manage Groups                    [×]  │
│                                        │
│  ┌──────────────────────────────────┐  │
│  │ 🏠 Home       [↑][↓] [Rename] [🗑]│  │
│  │ 🔧 Infra      [↑][↓] [Rename] [🗑]│  │
│  │ 📦 Apps       [↑][↓] [Rename] [🗑]│  │
│  └──────────────────────────────────┘  │
│                                        │
│  ┌─────────────────┐ ┌────────────┐   │
│  │ New group name  │ │ + Create   │   │
│  └─────────────────┘ └────────────┘   │
└────────────────────────────────────────┘
```

**Interactions:**
- **Create**: text input + "Create" button → `POST /api/v1/groups`
- **Rename**: inline text input per row, save on blur or Enter → `PUT /api/v1/groups/{id}` with `{ name }`
- **Reorder (↑/↓)**: swap `sort_order` values between adjacent rows → two `PUT` calls
- **Delete**: single click with no confirmation (groups are cheap to recreate; services are unaffected) → `DELETE /api/v1/groups/{id}`, then refetch groups in dashboard

### 3b. Trigger placement

Add a **"Manage Groups"** menu item to the existing "+ Add" dropdown on the dashboard (alongside "Service" and "Quick Link"). This reuses the dropdown pattern already in place and avoids cluttering the sort pill controls.

Icon: folder/tag SVG, consistent with existing menu item icon style.

### 3c. `DiscoveryPanel` — pass groups + include group_id

`DiscoveryPanel` currently accepts only `on_added: Callback<()>`. We add:
```rust
#[prop(default = vec![])] groups: Vec<GroupItem>
```
- Forward `groups` to the `EditModal` it opens
- Include `group_id` in the POST body when saving a discovered service

### 3d. Dashboard wiring

- Import `GroupsModal` and `modal_groups`
- Add `(show_groups_modal, set_show_groups_modal)` signal
- Mount `<GroupsModal>` with `visible`, `on_close`, `on_saved` (refetches groups + services)
- Add "Manage Groups" item to the "+ Add" dropdown
- Pass `resolve_groups(&groups)` to `DiscoveryPanel` as a `groups` prop

---

## 4. Implementation Steps

1. Create `crates/vexboard-frontend/src/components/modal_groups.rs`
   - Fetches groups internally via `LocalResource`
   - Inline rename (signal per row editing state)
   - Create form at bottom
   - ↑/↓ reorder via PUT (swap sort_order)
   - Delete via DELETE
   - Calls `on_saved: Callback<()>` after any mutation so dashboard refetches

2. Register in `crates/vexboard-frontend/src/components/mod.rs`

3. Modify `crates/vexboard-frontend/src/components/discovery_panel.rs`
   - Add `groups: Vec<GroupItem>` prop
   - Pass `groups` to `EditModal`
   - Add `"group_id": data.group_id` to the POST body in `on_save`

4. Modify `crates/vexboard-frontend/src/pages/dashboard.rs`
   - Import `GroupsModal`
   - Add `show_groups_modal` signal
   - Mount `<GroupsModal>`
   - Add "Manage Groups" to "+ Add" dropdown
   - Pass `groups=resolve_groups(&groups)` to `<DiscoveryPanel>`

---

## 5. Dependencies

No new dependencies. All patterns use:
- `leptos::prelude::*` (signals, `LocalResource`, `Callback`, `spawn_local`)
- `gloo_net` for HTTP (already in use across all frontend components)
- `serde_json` for JSON bodies (already in use)

Context7 not required (no new external dependencies).

---

## 6. Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo build --release --bin vexboard-server`

`cargo test --workspace` — no frontend tests exist; backend has no group-related unit tests to add.

---

## 7. Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Reorder with only 1 group — ↑/↓ buttons shown but no adjacent row | Disable ↑ on first item, ↓ on last item |
| Deleting a group leaves services with stale group_id | Backend keeps group_id on services; they fall to "Ungrouped" in Group sort mode — acceptable |
| Inline rename UX — losing edits on click away | Save on blur (standard pattern) |
| `GroupsModal` needs groups refetch after mutations AND dashboard also needs to refetch | `on_saved` callback triggers `groups.refetch()` in dashboard |
