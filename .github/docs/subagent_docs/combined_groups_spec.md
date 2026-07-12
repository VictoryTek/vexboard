# Combined Groups (Services + Quick Links) — Specification

**Feature:** Unify service groups and quick-link groups into one group concept, so a single group can contain services, quick links, or both — rendered as a services row above a quick-links row.
**Date:** 2026-07-11
**Phase:** 1 — Research & Specification

---

## 1. Current State Analysis

Groups are two fully independent systems today, all the way down the stack:

### Database (two separate tables, independent PK spaces)

`crates/vexboard-server/src/db/migrations/001_init.sql` (+ `004_group_color.sql`):
```sql
CREATE TABLE IF NOT EXISTS groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    icon        TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);
-- 004_group_color.sql: ALTER TABLE groups ADD COLUMN color TEXT;

CREATE TABLE IF NOT EXISTS services (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    ...
    group_id         INTEGER REFERENCES groups(id) ON DELETE SET NULL,
    sort_order       INTEGER NOT NULL DEFAULT 0,
    ...
);
```

`crates/vexboard-server/src/db/migrations/006_quick_link_groups.sql`:
```sql
CREATE TABLE IF NOT EXISTS quick_link_groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    icon        TEXT,
    color       TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE quick_links ADD COLUMN group_id INTEGER REFERENCES quick_link_groups(id) ON DELETE SET NULL;
```

Because `services.group_id` FKs to `groups.id` and `quick_links.group_id` FKs to `quick_link_groups.id`, a service and a quick link can never reference the same group row today.

### Backend

- `crates/vexboard-server/src/db/models.rs`: `Group` (struct, lines ~4-12) and `QuickLinkGroup` (lines ~182-190) are structurally identical (`id, name, icon, color, sort_order, created_at`) but distinct Rust types with distinct CRUD DTOs (`CreateGroup`/`UpdateGroup` vs `CreateQuickLinkGroup`/`UpdateQuickLinkGroup`).
- Routes (`crates/vexboard-server/src/api/mod.rs`): `/api/v1/groups` (`api/groups.rs`) and `/api/v1/quick-link-groups` (`api/quick_link_groups.rs`) are separate CRUD route sets.
- `list_groups` / `list_quick_link_groups` each return a flat `Vec<...>` — there is no nested/joined "group with members" endpoint; the frontend fetches 4 flat lists (`groups`, `quick_link_groups`, `services`, `quick_links`) and joins them client-side.

### Frontend

- `crates/vexboard-frontend/src/pages/dashboard/mod.rs` renders two independent sibling sections unconditionally:
  ```rust
  <ServiceGrid services=services groups=groups sort_mode=sort_mode ... />
  <QuickLinksSection quick_links=quick_links groups=quick_link_groups sort_mode=sort_mode ... />
  ```
- `service_grid.rs` and `quick_links_section.rs` each independently build per-group sections (`item.group_id == Some(group.id)`, plus an "Ungrouped" bucket) and each implement their own drag-drop reorder logic — ~150 lines of near-duplicated code per file, only exercised when the page-level `SortMode::Group` toggle is active.
- Two separate management modals: `modal_groups.rs` (`GroupsModal`, posts to `/api/v1/groups`) and `modal_quick_link_groups.rs` (`QuickLinkGroupsModal`, posts to `/api/v1/quick-link-groups`), opened from two separate dropdown menu items in `dashboard/mod.rs`.
- `GroupItem { id, name }` (defined in `modal_edit.rs`) is a shared *dropdown-option shape* used by both the service-edit and quick-link-edit modals — it is not evidence of a shared table; each context populates it from a different source list (`resolve_groups` vs `resolve_quick_link_groups`, `dashboard/mod.rs`).

### Sort order

- `sort_order` is an independent `INTEGER NOT NULL DEFAULT 0` column on all 4 tables.
- New service-groups get an incrementing `sort_order` client-side (`modal_groups.rs do_create`). New quick-link-groups currently always POST `sort_order: 0` (`modal_quick_link_groups.rs do_create`) — a pre-existing minor inconsistency, noted here but **out of scope** for this feature (not to be silently fixed per Surgical Changes principle; flagged for the user separately if desired).
- Reorder endpoints (`PATCH /api/v1/services/reorder`, `PATCH /api/v1/quick-links/reorder`) operate on independent `sort_order` sequences per entity type. Within a combined group, the existing per-entity `sort_order` (plus the existing alphabetical tiebreak) is sufficient to order the services row and the quick-links row independently — no new unified ordering concept is needed.

---

## 2. Problem Definition

The user wants groups to be a single concept: a group may contain services only, quick links only, or both. When a group contains both, services render as a row of cards above the quick links within that same group container. This requires:

1. Collapsing two independent group tables/types into one, since a service and a quick link must be able to reference the *same* group row.
2. Replacing the two separate flat list-and-join CRUD systems with one.
3. Replacing the two independently-coded frontend section builders with one shared component that, per group, renders a services row then a quick-links row.
4. Merging the two "manage groups" modals into one.
5. Migrating existing data (both tables' rows) into the unified table without breaking existing FK references or the `UNIQUE(name)` constraint on either table.

---

## 3. Proposed Solution Architecture

### 3a. Database migration — unify into one `groups` table

New migration `crates/vexboard-server/src/db/migrations/007_unify_groups.sql`:

1. Add a `color` column check — `groups` already has `color` from `004_group_color.sql`; no change needed there.
2. Insert all rows from `quick_link_groups` into `groups`, remapping ids to continue after the current max `groups.id`, and handling name collisions (see 3b).
3. Add `quick_links.new_group_id` as a temporary column, populate it via a remap table (old `quick_link_groups.id` → new `groups.id`), then:
   - Drop the old `quick_links.group_id` column (SQLite requires table-rebuild for column drop — use the standard "create new table, copy data, drop old, rename" pattern already established by prior migrations in this repo, or `ALTER TABLE ... DROP COLUMN` if the sqlx/SQLite version in use supports it — confirm SQLite version support before writing raw SQL).
   - Rename `new_group_id` to `group_id`, add `REFERENCES groups(id) ON DELETE SET NULL`.
4. Drop `quick_link_groups` table.

**Name collision handling (3b):** Since both `groups.name` and `quick_link_groups.name` are independently `UNIQUE`, identical names existing in both tables today (e.g. a "Media" service group and a separate "Media" quick-link group) would collide once merged. The migration must detect duplicates and suffix the incoming `quick_link_groups` name (e.g. `"Media (2)"`) before insert, OR — if the user already intends same-named groups to *become* the same combined group — merge them into one row instead of renaming. **This needs a decision before implementation**: does a pre-existing same-named pair of groups become one combined group automatically, or do both survive as distinct groups with a disambiguated name? Default proposed here: **keep them distinct** (safer, no silent behavior change to existing dashboards) — auto-suffix on collision. Flag this default to the user in Phase 2 kickoff in case they'd prefer auto-merge.

### 3b. Backend

- Delete `QuickLinkGroup` struct, `CreateQuickLinkGroup`, `UpdateQuickLinkGroup`, and the `api/quick_link_groups.rs` route module.
- `quick_links` handlers (`api/quick_links.rs`) switch their `group_id` validation/FK target from `quick_link_groups` to `groups`.
- `groups.rs` CRUD is otherwise unchanged (`Group`, `CreateGroup`, `UpdateGroup` already match the target shape).
- No new "nested" endpoint is required — frontend continues to fetch flat `groups`, `services`, `quick_links` lists and join client-side (consistent with current architecture; adding a nested endpoint would be a bigger change than needed here per Simplicity First).

### 3c. Frontend

- New shared component, e.g. `crates/vexboard-frontend/src/pages/dashboard/group_section.rs`, replacing the per-type section-building logic currently duplicated in `service_grid.rs` and `quick_links_section.rs`. For each group (in `sort_order` order, with the existing alphabetical tiebreak), it renders:
  - A group header (name, icon, color pill) — unchanged visual style from today.
  - A services row: `ServiceCard`s for services with `group_id == Some(group.id)`, sorted/reordered exactly as `service_grid.rs` does today.
  - A quick-links row below it: `QuickLinkCard`s for quick links with `group_id == Some(group.id)`, sorted/reordered exactly as `quick_links_section.rs` does today.
  - If a group has services but no quick links (or vice versa), only the populated row renders — no empty row/placeholder.
- `ServiceGrid` and `QuickLinksSection` are retired as separate group-mode renderers; drag-and-drop reorder logic for each row type is preserved as-is, just relocated into the shared component (moving code, not rewriting the reorder algorithm).
- Non-`Group` sort modes (`AZ`, `Source`) are unaffected — services and quick links continue to render in their existing separate flat/by-source sections in those modes; only `SortMode::Group` rendering is restructured. (Open question for Phase 2, see Risks: should `AZ`/`Source` modes remain fully separate, or is unifying groups expected to also change those views? Assumed **no change** here — user only mentioned "when grouped together".)
- `modal_groups.rs` (`GroupsModal`) becomes the single "Manage Groups" modal; `modal_quick_link_groups.rs` and its dropdown menu entry are deleted. Group create/edit no longer needs a "kind" picker — a group is just a group; whether it ends up holding services, quick links, or both is determined entirely by what gets assigned to it via the existing service-edit / quick-link-edit modals' group dropdown (both already share the `GroupItem` shape and will now point at the same source list).
- `dashboard/mod.rs`: drop the second `quick_link_groups` `LocalResource` and the second dropdown menu item; `resolve_groups`/`resolve_quick_link_groups` collapse into one resolver.

### 3d. Migration/rollout order (Phase 2 implementation sequence)

1. DB migration (007) — data unification.
2. Backend: remove `quick_link_groups` route/model, repoint `quick_links` FK validation.
3. Frontend: new shared `GroupSection` component; wire into `dashboard/mod.rs` for `SortMode::Group`; remove old `ServiceGrid`/`QuickLinksSection` group-mode branches (their non-group-mode rendering, if any, stays — confirm from code whether those components handle non-Group modes internally or if that's already handled elsewhere in `dashboard/mod.rs` before deciding whether to delete the files outright or just their group-mode paths).
4. Frontend: merge management modals, update dropdown menu.
5. Update OpenAPI spec / any generated docs referencing `quick-link-groups` endpoints.

---

## 4. Dependencies

No new external crates or libraries required — this is entirely internal schema/route/component consolidation using the existing Axum/sqlx/Leptos stack already in the workspace. Context7 lookup not applicable (Dependency Policy exemption: "Internal code changes with no new dependencies").

---

## 5. Configuration Changes

None. No new config keys or env vars.

---

## 6. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Data loss / broken FK during SQLite column-drop migration | Use the repo's established "rebuild table" migration pattern (check prior migrations for precedent); test against a copy of a real `vexboard.db` before applying; migration must be reviewed in Phase 3 with explicit attention to irreversibility. |
| Name collisions between existing `groups` and `quick_link_groups` rows | Default: auto-suffix duplicate incoming names during migration (see 3b). Needs explicit user confirmation before Phase 2 if auto-merge is preferred instead. |
| Duplicated drag-drop reorder code merged incorrectly, causing reorder regressions in one row type | Move (not rewrite) the existing per-type reorder logic into the shared component; Phase 3 review must manually verify both services and quick-links drag-drop still work post-merge (cannot be verified by `cargo test` alone — flag for manual/UI verification since preflight is backend-only). |
| Removing `quick_link_groups` breaks anything still referencing it (OpenAPI docs, seed scripts, other migrations) | Grep the full repo for `quick_link_groups` and `QuickLinkGroup` before deletion in Phase 2 to catch all references. |
| Existing dashboards relying on `SortMode::Group` mid-migration (server restart during migration) | Migration runs on server startup as part of existing migration runner (assumed, matching existing `NNN_*.sql` pattern) — no user-facing action needed, but confirm this assumption against `db/mod.rs`'s migration-running code in Phase 2. |

**Open questions for user (recommend resolving before/at Phase 2 kickoff, not silently assumed):**
1. On migration, if a service-group and a quick-link-group already share the same name, should they merge into one combined group, or stay distinct with a disambiguated name? (Spec defaults to **stay distinct**.)
2. Should groups with zero services and zero quick links still display as an empty container in Group sort mode, or be hidden entirely? (Not addressed above — current behavior for empty groups should be preserved; confirm no change intended.)
