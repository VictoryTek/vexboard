# Quick Link Grouping & Reordering — Spec

## 1. Current State Analysis

**Services** (reference implementation, already working):
- `groups` table (`crates/vexboard-server/src/db/migrations/001_init.sql:4-10`, color added in `004_group_color.sql`): `id, name, icon, color, sort_order, created_at`.
- `services.group_id` FK + `services.sort_order` (`001_init.sql:12-28`).
- `Group`, `Service`, `ReorderItem` models in `crates/vexboard-server/src/db/models.rs:5-31,182-186`.
- Group CRUD: `crates/vexboard-server/src/api/groups.rs` (`GET/POST /api/v1/groups`, `PUT/DELETE /api/v1/groups/{id}`).
- Service reorder: `PATCH /api/v1/services/reorder` in `crates/vexboard-server/src/api/services.rs:522-606`, takes `Vec<ReorderItem>`, transactional per-id `sort_order` update, writes an audit log entry.
- Frontend: `ServiceResponse` includes `group_id`/`sort_order` (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:24-38`); `SortMode` enum `AZ | Source | Group` with a toggle in `DashboardPage` (`mod.rs:70-176`); `ServiceGrid` (`crates/vexboard-frontend/src/pages/dashboard/service_grid.rs`) renders per-mode sections, each with native HTML5 drag-and-drop (`draggable="true"` + `on:dragstart/dragover/dragleave/drop/dragend`) wired through `drag_src_idx`/`drag_over_idx` (flat mode) or `section_drag_src`/`section_drag_over: RwSignal<Option<(String, usize)>>` (sectioned mode), plus a "reset to A-Z" button per section. Drop handlers refetch, reorder in-memory, recompute contiguous `sort_order`, PATCH `/reorder`, refetch again.
- `GroupsModal` (`crates/vexboard-frontend/src/components/modal_groups.rs`) is the admin group-management UI: create/rename/recolor/delete/reorder groups (up/down buttons swapping `sort_order`).

**Quick links** (target of this feature) — currently a flat, ungrouped, unorderable-by-drag list:
- `quick_links` table (`001_init.sql:50-59`): `id, title, url, icon, description, sort_order, created_at, updated_at`. `sort_order` exists in the DB but is set once at creation and never exposed to or mutated by the frontend beyond initial insert order.
- `QuickLink`/`CreateQuickLink`/`UpdateQuickLink` models: `crates/vexboard-server/src/db/models.rs:143-180`. No `group_id` field anywhere.
- API: `crates/vexboard-server/src/api/quick_links.rs` — list/create/update/delete only. **No reorder endpoint.**
- Frontend `QuickLinkResponse` (`mod.rs:47-54`) has no `group_id`/`sort_order` field — the backend's `sort_order` value is silently dropped on the wire today.
- `QuickLinksSection` (`crates/vexboard-frontend/src/pages/dashboard/quick_links_section.rs`) renders links in raw API order in one flat CSS grid — no drag handlers, no grouping, no sort-mode toggle.
- `QuickLinkModal` (`crates/vexboard-frontend/src/components/quick_link_modal.rs`) form: title/url/description/icon only — no group selector.

**Decision (confirmed with user):** quick links get their **own independent group table** (`quick_link_groups`), separate from the services `groups` table — not a shared FK. This mirrors the services grouping feature structurally but keeps the two grouping schemes independent, since a service group and a quick-link group are different concepts even if they share a name.

## 2. Problem Definition

Quick links currently cannot be grouped into named sections or manually reordered via drag-and-drop, unlike services. Add parity: an admin-manageable `quick_link_groups` table, a `Group` sort mode for quick links, and native-HTML5 drag-and-drop reordering (flat and within-group), reusing the same interaction patterns already proven in `service_grid.rs`.

## 3. Proposed Solution Architecture

### Backend

**New migration** `crates/vexboard-server/src/db/migrations/006_quick_link_groups.sql`:
```sql
CREATE TABLE quick_link_groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    icon TEXT,
    color TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE quick_links ADD COLUMN group_id INTEGER REFERENCES quick_link_groups(id) ON DELETE SET NULL;
```
(Mirrors `groups`/`services.group_id` exactly; SQLite requires `ALTER TABLE ADD COLUMN` for the FK rather than inline in `001_init.sql` since that file is already applied historically.)

**Models** (`crates/vexboard-server/src/db/models.rs`):
- New `QuickLinkGroup` struct (id, name, icon, color, sort_order, created_at) — identical shape to `Group`.
- New `CreateQuickLinkGroup` / `UpdateQuickLinkGroup` DTOs — identical shape to `CreateGroup`/`UpdateGroup`.
- Add `pub group_id: Option<i64>` to `QuickLink`, `CreateQuickLink`, `UpdateQuickLink` (same position/pattern as `Service`'s `group_id`).
- Reuse the existing `ReorderItem { id, sort_order }` DTO for quick-link reordering (already generic, no new type needed).

**New API module** `crates/vexboard-server/src/api/quick_link_groups.rs`, structurally identical to `groups.rs`:
- `GET /api/v1/quick-link-groups` (read router)
- `POST /api/v1/quick-link-groups`, `PUT/DELETE /api/v1/quick-link-groups/{id}` (admin router)
- Audit actions: `quick_link_group.create` / `.update` / `.delete`.

**`quick_links.rs` changes**:
- Update all SQL (`SELECT`/`INSERT`/`UPDATE`) to include `group_id`.
- Add `PATCH /api/v1/quick-links/reorder` (`reorder_quick_links`), copied from `services.rs:522-606` — transactional loop over `Vec<ReorderItem>` updating `sort_order` on `quick_links`, audit action `quick_link.reorder`.

**Router wiring** (`crates/vexboard-server/src/api/mod.rs`):
- `read_router`: add `.nest("/api/v1/quick-link-groups", quick_link_groups::read_router())`.
- `admin_router`: add `.nest("/api/v1/quick-link-groups", quick_link_groups::admin_router())`; add the reorder route to the existing `/api/v1/quick-links` admin nest (PATCH `/reorder`, same nesting pattern as `services::admin_router()` already does internally for `/reorder`).

**OpenAPI** (`crates/vexboard-server/src/api/openapi.rs`): register the 4 new `quick_link_groups` handlers and `reorder_quick_links`, same list style as existing `groups`/`quick_links` entries (lines 42-49).

### Frontend

**`mod.rs`**:
- Add `group_id: Option<i64>` and `sort_order: i64` fields to `QuickLinkResponse`.
- Add `QuickLinkGroupResponse` struct, identical shape to `GroupResponse` (`id, name, color`).
- Add `fetch_quick_link_groups()` (mirrors `fetch_groups()`).
- Add `reorder_quick_links(items: Vec<(i64, i64)>)` (mirrors `reorder_services`, PATCHes `/api/v1/quick-links/reorder`).
- Add a `quick_link_sort_mode: (ReadSignal<SortMode>, WriteSignal<SortMode>)` — quick links get their own independent sort-mode state (AZ/Group; **no "Source" mode**, since quick links have no discovery source) so toggling service sort mode doesn't affect quick links and vice versa. Reuse the `SortMode` enum but only render `AZ`/`Group` buttons in the quick-links UI.
- Add drag-state signals for quick links: `ql_drag_src_idx`/`ql_drag_over_idx` (flat) and `ql_section_drag_src`/`ql_section_drag_over: RwSignal<Option<(String, usize)>>` (grouped) — separate signals from the service ones so dragging a service card can't interfere with quick-link drag state.
- Wire a "Quick Link Group" entry into the existing "+ Add" dropdown menu (alongside "Service"/"Quick Link"/"Manage Groups") that opens a new `show_quick_link_groups_modal` signal, OR extend the existing group management entry — see Implementation Steps below for the concrete choice.

**New component** `crates/vexboard-frontend/src/components/modal_quick_link_groups.rs`: copy of `modal_groups.rs`, retargeted at `/api/v1/quick-link-groups` endpoints. (Duplication is intentional and matches the project's existing pattern of near-duplicated group/reorder logic between sort modes in `service_grid.rs`; introducing a shared generic component is out of scope per the Simplicity-First principle — this is a mechanical endpoint swap, not shared logic worth abstracting.)

**`quick_links_section.rs`** — rewritten to mirror `service_grid.rs`'s structure:
- Accept new props: `groups: LocalResource<Vec<QuickLinkGroupResponse>>`, `sort_mode: ReadSignal<SortMode>`, the 4 quick-link drag signals.
- When `sort_mode == SortMode::Group`: build sections per group (members filtered by `group_id`, sorted by `sort_order` then title; "Ungrouped" section for links with no/unknown `group_id`), each card wrapped in a `draggable="true"` div with the same dragstart/dragover/dragleave/drop/dragend handlers as `service_grid.rs`'s group-mode branch (lines 206-274), calling `reorder_quick_links` instead of `reorder_services`. Include the same per-section "reset to A-Z" button.
- When `sort_mode == SortMode::AZ` (or default): flat grid, sorted by `sort_order` then title, each card wrapped with the flat-mode drag handlers (mirrors `service_grid.rs:464-517`), calling `reorder_quick_links`.
- No "Source" section variant (quick links have no discovery source concept).

**`DashboardPage` (`mod.rs`)**: add a small sort-mode toggle (AZ/Group only) above the Quick Links section, and a "Manage Quick Link Groups" entry in the "+ Add" dropdown (or a second dropdown item next to "Manage Groups") that opens `QuickLinkGroupsModal`. Pass the new `quick_links` group data + sort mode + drag signals into `QuickLinksSection`.

**`modal_quick_link_modal.rs` / edit flow**: add a group selector (dropdown) to `QuickLinkModal`, reusing the `GroupItem`-style pattern from `modal_edit.rs`'s group select — add `group_id: Option<i64>` to `QuickLinkFormData`, and thread it through `modals.rs`'s `on_save_link`/`on_edit_save` request bodies (mirrors how `EditFormData.group_id` already flows through service create/update in `modals.rs:21-38,85-102`).

## 4. Implementation Steps

1. Migration `006_quick_link_groups.sql` — create `quick_link_groups` table, add `group_id` column to `quick_links`.
2. Backend models: `QuickLinkGroup`, `CreateQuickLinkGroup`, `UpdateQuickLinkGroup`; add `group_id` to `QuickLink`/`CreateQuickLink`/`UpdateQuickLink`.
3. New `crates/vexboard-server/src/api/quick_link_groups.rs` (copy-adapt `groups.rs`).
4. Update `crates/vexboard-server/src/api/quick_links.rs`: include `group_id` in all queries; add `reorder_quick_links` handler + route.
5. Wire routers in `crates/vexboard-server/src/api/mod.rs`; register new/changed handlers in `openapi.rs`.
6. Frontend types/fetchers in `mod.rs`: `QuickLinkGroupResponse`, extend `QuickLinkResponse`, `fetch_quick_link_groups`, `reorder_quick_links`, new sort-mode + drag signals.
7. New `modal_quick_link_groups.rs` component (copy-adapt `modal_groups.rs`).
8. Rewrite `quick_links_section.rs` with grouped/flat rendering + drag-and-drop (copy-adapt relevant branches of `service_grid.rs`).
9. Add group selector to `QuickLinkFormData`/`QuickLinkModal`; thread `group_id` through `modals.rs` save/edit handlers.
10. Update `DashboardPage` (`mod.rs`) to render the quick-link sort toggle and "Manage Quick Link Groups" trigger, and pass new props into `QuickLinksSection`/modals.

## 5. Dependencies

None new — reuses existing Axum/sqlx/Leptos/gloo-net patterns already in the codebase. No Context7 lookup required (no new external library).

## 6. Configuration Changes

None. New migration is applied automatically via the existing sqlx migration runner at startup (same mechanism as `004_group_color.sql`/`005_dismissed_units.sql`).

## 7. Risks & Mitigations

- **Risk:** `ALTER TABLE ADD COLUMN ... REFERENCES` — confirm SQLite/sqlx accepts inline FK syntax in `ALTER TABLE ADD COLUMN` (it does; SQLite treats it as an unenforced-by-default but valid FK declaration, consistent with how `services.group_id` was presumably added — verify by checking whether `services.group_id` was added via `001_init.sql` inline (it was, per file read) — so this is the *first* `ALTER TABLE ADD COLUMN` with a FK in this codebase). Mitigation: test the migration runs cleanly against a fresh DB during Phase 3 review (`cargo test -p vexboard-server` will exercise migrations).
- **Risk:** Frontend duplication between `service_grid.rs` and `quick_links_section.rs` drag logic (~150 lines duplicated). Accepted per Simplicity-First principle — the existing codebase already triplicates this pattern within `service_grid.rs` itself (group/source/flat modes), so a fourth near-copy is consistent with established style, not a new smell. Do not introduce a shared abstraction unless asked.
- **Risk:** SQLx offline query cache (`.sqlx/` or `DATABASE_URL`) may need regeneration after query changes if compile-time checked. Mitigation: confirm `SQLX_OFFLINE`/`DATABASE_URL` handling before Phase 2 build validation (per Resource Constraints in CLAUDE.md).
