# FEAT-2 — Dismiss discovered services — Spec

## Current State Analysis

- `src/discovery/mod.rs` holds the in-memory `DiscoveryList` (`Arc<RwLock<Vec<DiscoveredUnit>>>`), refreshed by two background loops:
  - `src/discovery/systemd.rs::discover_units` — replaces all `source == "systemd"` entries each pass, filtering out only units already **claimed** (`services.systemd_unit` match).
  - `src/discovery/docker.rs::discover_containers` — replaces all `source == "docker"/"podman"` entries each pass, filtering out claimed units by `display_name`/`systemd_unit` match.
- Neither loop is aware of "dismissed" units — there is no such concept anywhere in the schema, backend, or frontend.
- `GET /api/v1/discovery` (`list_discovered`) returns the full in-memory list unfiltered.
- `POST /api/v1/discovery/refresh` (`trigger_refresh`) spawns both discovery passes in the background; already admin-protected via `admin_protected` router nest in `src/api/mod.rs:47`.
- Frontend `crates/vexboard-frontend/src/components/discovery_panel.rs` renders each `DiscoveredUnitFe` as a card with a single "Add" button. Settings page copy already claims users can "claim or dismiss" discovered services (per FEAT-2 description) but no dismiss control exists.
- Existing admin CRUD pattern (`src/api/quick_links.rs`) shows the house style: `read_router()`/`admin_router()` split, `sqlx::query` with manual error mapping to `(StatusCode, Json)`, `db::audit::insert(...)` on every mutation, `#[utoipa::path]` doc annotations.
- Migration pattern (`src/db/mod.rs::run_migrations`): each new migration is an idempotent `ALTER`/`CREATE TABLE IF NOT EXISTS` gated by a `pragma_table_info`/existence probe, applied unconditionally on top of `001_init.sql`. Next unused migration number is `005`.

## Problem Definition

Discovered-but-unclaimed systemd units and containers that a user does NOT want to add to the dashboard reappear on every discovery pass forever, since there is no persistence for "I looked at this and don't want it." This clutters the Discovered Services panel indefinitely.

## Proposed Solution

1. **Schema** — new table `dismissed_units (source, unit_name)`, unique on `(source, unit_name)`, so a dismissal is keyed the same way discovery/claim matching already works.
2. **Backend filtering** — after building the unclaimed list in `discover_units` / `discover_containers`, filter out any `(source, unit_name)` present in `dismissed_units`. This mirrors the existing "claimed" check style (one query per pass, in-memory dedupe) rather than adding an extra layer.
3. **API** — add to `src/discovery/mod.rs` (already nested under `admin_protected` in `src/api/mod.rs:47`, so no router wiring changes needed there):
   - `POST /api/v1/discovery/dismiss` — body `{ "source": String, "unit_name": String }` — inserts into `dismissed_units` (`INSERT OR IGNORE`), removes the matching entry from the in-memory `DiscoveryList` immediately (so the UI doesn't need a refresh), audit-logs `discovery.dismiss`.
   - `DELETE /api/v1/discovery/dismiss` — same body shape — removes the row (un-dismiss / "undo"), audit-logs `discovery.undismiss`. No dismissed-list UI is in scope for this feature per the spec; this endpoint exists for symmetry/future use and is cheap to add alongside the table, but is optional — see Implementation Steps for the minimal cut.
4. **Frontend** — in `discovery_panel.rs`, add a "Dismiss" (or "×") button next to "Add" on each discovered-unit card. On click, POST to `/api/v1/discovery/dismiss` with `{source, unit_name}`, then `units.refetch()` (or optimistically remove from the local resource — refetch is simpler and consistent with existing `on_added` pattern).

### Decision: skip the undismiss endpoint for v1?

The MASTER_PLAN fix text only asks for a `dismissed_units` table, `POST /dismiss` (+ `DELETE`), and filtering. I'll include both POST and DELETE since the plan explicitly calls for "add `POST /api/v1/discovery/dismiss` and `DELETE` endpoint (admin)" — this is in scope, not speculative. No "manage dismissed units" UI is requested, so the DELETE endpoint ships without a frontend consumer for now (documented, not dead — it's a supported API surface, consistent with other admin endpoints that predate their UI, e.g. audit log).

## Implementation Steps

1. Add `crates/vexboard-server/src/db/migrations/005_dismissed_units.sql`:
   ```sql
   CREATE TABLE IF NOT EXISTS dismissed_units (
       id          INTEGER PRIMARY KEY AUTOINCREMENT,
       source      TEXT NOT NULL,
       unit_name   TEXT NOT NULL,
       created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
       UNIQUE(source, unit_name)
   );
   ```
2. Wire it into `run_migrations` in `src/db/mod.rs` following the existing `CREATE TABLE IF NOT EXISTS` pattern used for `002_audit_log.sql` (unconditional `raw_sql`, no probe needed since `IF NOT EXISTS` makes it idempotent on its own — matches how `002` is handled, not the probe-gated `ALTER TABLE` pattern used for `003`/`004` since this is a new table, not a new column).
3. In `src/discovery/mod.rs`:
   - Add `DismissRequest { source: String, unit_name: String }` DTO (serde `Deserialize`, utoipa `ToSchema`).
   - Add `dismiss_unit` handler (`POST /dismiss`): `INSERT OR IGNORE INTO dismissed_units (source, unit_name) VALUES (?, ?)`, then `discoveries.write().await.retain(|u| !(u.source == source && u.unit_name == unit_name))`, then `db::audit::insert(..., "discovery.dismiss", ...)`.
   - Add `undismiss_unit` handler (`DELETE /dismiss`): `DELETE FROM dismissed_units WHERE source = ? AND unit_name = ?`, audit `discovery.undismiss`. (Does not need to re-add to the in-memory list — next scheduled/triggered discovery pass will pick it back up naturally.)
   - Register both routes in `router()`: `.route("/dismiss", post(dismiss_unit).delete(undismiss_unit))`.
4. In `src/discovery/systemd.rs::discover_units`: after computing `unclaimed`, fetch dismissed systemd unit names (`SELECT unit_name FROM dismissed_units WHERE source = 'systemd'`) once per pass and filter them out before pushing into `unclaimed` (same spot as the existing `claimed` check, ~line 133).
5. In `src/discovery/docker.rs::discover_containers`: same approach — fetch dismissed `(docker|podman)` unit names once per pass (both sources, since a single pass covers all sockets) and skip matching containers in `discover_from_socket`'s claimed-check block (~line 185).
6. Frontend `discovery_panel.rs`:
   - Add `async fn dismiss_unit(source: String, unit_name: String)` POST helper.
   - Add a small "Dismiss" button beside the existing "Add" button on each card; on click, call the helper then `units.refetch()`.

## Dependencies

No new external dependencies. No Context7 lookup required — this uses only sqlx/axum patterns already established in the codebase (Dependency Policy exemption: "Internal code changes with no new dependencies").

## Configuration Changes

None.

## Risks & Mitigations

- **Race between dismiss and refresh**: a discovery pass could re-add a unit to the in-memory list concurrently with a dismiss call. Mitigated by filtering at the DB level on every future pass — worst case the unit reappears for one refresh cycle, self-heals next pass.
- **Migration ordering**: must append as `005_dismissed_units.sql` and call it unconditionally (`CREATE TABLE IF NOT EXISTS`) after `004_group_color.sql` in `run_migrations`, consistent with `002_audit_log.sql`'s unconditional/idempotent style — avoids the probe-based branching used for column additions, which doesn't apply to new tables.
- **Frontend button clutter**: keep the Dismiss button visually secondary (text/ghost style) so "Add" remains the primary action, matching existing card layout conventions.

## Files

- `crates/vexboard-server/src/db/migrations/005_dismissed_units.sql` (new)
- `crates/vexboard-server/src/db/mod.rs` (wire migration)
- `crates/vexboard-server/src/discovery/mod.rs` (DTO, handlers, router)
- `crates/vexboard-server/src/discovery/systemd.rs` (filter dismissed)
- `crates/vexboard-server/src/discovery/docker.rs` (filter dismissed)
- `crates/vexboard-frontend/src/components/discovery_panel.rs` (Dismiss button + fetch helper)
