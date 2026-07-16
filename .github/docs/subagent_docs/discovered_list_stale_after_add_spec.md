# Spec: Discovered list not updated reactively after adding a service

## Current State Analysis

- Frontend: `crates/vexboard-frontend/src/components/discovery_panel.rs`
  - `units` (line ~95) is a `LocalResource::new(fetch_discovered_units)` backing the Discovered page list, sourced from `GET /api/v1/discovery`.
  - `on_save` (lines ~101-136), invoked when the add-service form is submitted:
    1. `POST /api/v1/services` to create the service (line ~124).
    2. `POST /api/v1/discovery/refresh` to trigger a rescan (line ~127-129) — fire-and-forget, does not await scan completion.
    3. Calls `units.refetch()` (line ~133).
  - `dismiss_unit` (lines ~76-81) follows the same `refetch()`-after-mutation pattern and works correctly today.

- Backend:
  - `POST /api/v1/services` → `create_service` (`crates/vexboard-server/src/api/services.rs:214-333`): inserts the new row into the `services` table but never touches `state.discoveries` (the shared in-memory `DiscoveryList`).
  - `POST /api/v1/discovery/refresh` → `trigger_refresh` (`crates/vexboard-server/src/discovery/mod.rs:82-124`): spawns two background tasks (`systemd::discover_units`, `docker::discover_containers`) and returns `202 Accepted` immediately, without waiting for them to finish. Those tasks are what eventually rebuild `state.discoveries` and drop the now-claimed unit.
  - `POST /api/v1/discovery/dismiss` → `dismiss_unit` (`discovery/mod.rs:140-185`) is the working reference pattern: after the DB write succeeds, it synchronously does `discoveries.retain(|u| !(u.source == ... && u.unit_name == ...))` on `state.discoveries` before responding `200`. Because the mutation is synchronous and complete before the response returns, the frontend's subsequent `units.refetch()` reliably reflects the removal.

## Problem Definition

`create_service` does not remove the newly-claimed unit from `state.discoveries`. The client's immediate `units.refetch()` (called right after `POST /api/v1/services` and firing off `POST /api/v1/discovery/refresh`) races the background discovery scan and normally returns before the scan finishes, so the just-added service still appears in the Discovered list. It only disappears once the background scan completes and the user manually reloads/refetches, which resolves the race.

## Proposed Solution

Mirror the `dismiss_unit` pattern in `create_service`: after the service row is successfully inserted, synchronously remove the matching entry from `state.discoveries` (matched on `source == payload.discovery_source` and `unit_name == payload.systemd_unit`) before the HTTP response is returned. This makes the removal deterministic and race-free, independent of the background rescan, and requires no frontend changes since `units.refetch()` already runs after the `POST /api/v1/services` call resolves.

The existing `POST /api/v1/discovery/refresh` call remains as-is (still useful for catching newly-appeared units), it just no longer needs to be the mechanism that removes the claimed unit.

## Implementation Steps

1. In `crates/vexboard-server/src/api/services.rs`, inside `create_service`'s `Ok(r) => { ... }` branch (after `let new_id = ...`), acquire `state.discoveries.write().await` and `retain` out any entry where `u.source == payload.discovery_source.as_deref().unwrap_or_default()` (or equivalent `Option<String>` comparison) `&& Some(u.unit_name.as_str()) == payload.systemd_unit.as_deref()`. Only do this when `payload.systemd_unit` is `Some(..)` (a manually-added service with no discovery link has nothing to remove).
2. Drop the write lock before continuing (match `dismiss_unit`'s `drop(discoveries);` style, or let it go out of scope before the `tokio::spawn` for probing, for consistency/clarity — not strictly required for correctness since it's a different lock than any used inside the probe task, but keep the lock scope tight).
3. No frontend changes needed.

## Dependencies

None — no new external dependencies. `state.discoveries` (`DiscoveryList = Arc<RwLock<Vec<DiscoveredUnit>>>`) is already part of `AppState`.

## Configuration Changes

None.

## Risks and Mitigations

- Risk: `payload.discovery_source` / `payload.systemd_unit` are both `Option<String>`; a mismatch in comparison (e.g. comparing `Option<String>` to `String` incorrectly) could silently fail to remove the entry. Mitigation: match `DiscoveredUnit.source: String` / `DiscoveredUnit.unit_name: String` fields precisely against the unwrapped `Option` values, only running the retain when both are `Some`.
- Risk: manually-added services (no `systemd_unit`/`discovery_source`) should not attempt any match — guarded by the `Some(..)` check in step 1.
- No behavior change to `dismiss_unit`, `trigger_refresh`, or the frontend.

## Approved Validation Commands (from CLAUDE.md, Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
