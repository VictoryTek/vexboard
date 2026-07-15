# Quick Link Group Clearing + Manage Groups Modal Desync — Spec

**Date:** 2026-07-15
**Phase:** 1 — Research & Specification

## Reported Symptoms

1. Services and quick links sort correctly into previously-assigned groups (`SortMode::Group` works).
2. The "Manage Groups" modal shows no groups, even though those same groups are clearly in use for sorting.
3. Selecting "— No Group —" when editing a quick link does not actually remove it from its group. (Confirmed by user: services clear correctly today; only quick links are affected.)

## Current State Analysis

### Issue A — quick links can't be cleared to "No Group" (confirmed root cause)

`UpdateQuickLink.group_id` (`crates/vexboard-server/src/db/models.rs:203`) is a plain `Option<i64>`:

```rust
pub struct UpdateQuickLink {
    pub title: Option<String>,
    pub url: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub group_id: Option<i64>,
    pub sort_order: Option<i64>,
}
```

This is the exact bug already fixed for `UpdateService.group_id` and `UpdateGroup.{icon,color}` in the prior `clear-to-null-fields` change (`crates/vexboard-server/src/db/models.rs:83-101`, using the `deserialize_some` double-`Option` helper) — `UpdateQuickLink.group_id` was missed by that change.

With plain `Option<i64>` deserialization, a JSON body with `"group_id": null` (key present, explicit null) is indistinguishable from the key being omitted entirely — both deserialize to `None`. The handler at `crates/vexboard-server/src/api/quick_links.rs:148`:

```rust
let group_id = payload.group_id.or(existing.group_id);
```

therefore always falls back to `existing.group_id` whenever the payload's `group_id` is `None`, so an explicit "clear the group" request is silently a no-op.

The frontend already sends the semantically-correct wire format: `crates/vexboard-frontend/src/pages/dashboard/modals.rs` (`on_edit_save` for quick links) always includes `"group_id": data.group_id`, and `crates/vexboard-frontend/src/components/quick_link_modal.rs:151-155` sets `selected_group_id` to `None` when "— No group —" is chosen, which serializes to JSON `null`. No frontend change is needed for this issue — it is purely a server-side deserialization gap, identical in shape to the already-fixed `UpdateService.group_id` case.

### Issue B — Manage Groups modal shows stale/empty data

`GroupsModal` (`crates/vexboard-frontend/src/components/modal_groups.rs`) fetches its own independent copy of the group list:

```rust
async fn fetch_groups_internal() -> Vec<GroupEntry> { ... }
...
pub fn GroupsModal(...) -> impl IntoView {
    let groups = LocalResource::new(fetch_groups_internal);
    ...
```

This is a **second, independent** `LocalResource` from the one the rest of the dashboard uses (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:139`, `let groups = LocalResource::new(|| async move { fetch_groups().await... });`), hitting the same `/api/v1/groups` endpoint but as a completely separate fetch with its own lifecycle.

`GroupsModal` is mounted unconditionally in `DashboardModals` (`crates/vexboard-frontend/src/pages/dashboard/modals.rs:60-64`) — not lazily re-created per open like the other modals (`EditModal`/`QuickLinkModal`, which use `show_modal.get().then(...)` to build a fresh instance on every open). This means:

- `GroupsModal`'s private `groups` resource is created exactly once, whenever `DashboardModals` first mounts (page load), and is **never** re-fetched afterward except from inside the modal's own `do_create`/`do_rename`/`do_delete`/`do_move` handlers (each calls `groups.refetch()` after its own mutation).
- If that one initial fetch ever returns stale, empty, or otherwise diverges from the shared `groups` resource (e.g. any transient race at boot, or simply having been fetched before other session state settled), there is no mechanism to ever resync it — the modal will keep showing the wrong data indefinitely, even though the shared resource (used correctly for sorting) is healthy.

This dual-source-of-truth architecture is the structural bug: two independent client caches of the same server list, one of which has no path back to consistency once it diverges. The fix is to have exactly one source of truth, refreshed on every relevant occasion.

`GroupEntry` (private to `modal_groups.rs`) duplicates fields already present on `GroupResponse` (`crates/vexboard-frontend/src/pages/dashboard/mod.rs:94-99`) except `sort_order`, which `GroupResponse` currently lacks but `GroupsModal`'s reorder buttons (`do_move`) and create-flow (`do_create`'s `next_order` calculation) require. `GroupEntry.icon` is already marked `#[allow(dead_code)]` in the modal — it is not used anywhere in `modal_groups.rs`'s rendering.

## Problem Definition

- A: `Option<T>` cannot distinguish "field omitted" from "field explicitly null" for `UpdateQuickLink.group_id`, so `PUT /api/v1/quick-links/{id}` with `{"group_id": null}` cannot clear a quick link's group.
- B: `GroupsModal` maintains its own independent, load-once copy of the groups list instead of sharing the dashboard's single `groups` resource, so it can permanently diverge from what the rest of the UI shows and has no way to self-correct.

## Proposed Solution

### A — apply the existing double-`Option` pattern to `UpdateQuickLink.group_id`

Mirror exactly what was already done for `UpdateService.group_id` in the prior fix, reusing the existing `deserialize_some` helper already defined in `models.rs` (no new helper needed).

### B — consolidate `GroupsModal` onto the shared `groups` resource, and refetch on open

- Add `sort_order: i64` to `GroupResponse` (`dashboard/mod.rs`) so it carries everything `GroupsModal` needs.
- Change `GroupsModal` to accept `groups: LocalResource<Vec<GroupResponse>>` as a prop (same resource `DashboardModals`/`DashboardPage` already own and pass around), instead of constructing its own `LocalResource`/`fetch_groups_internal`/`GroupEntry`.
- Add an `Effect` inside `GroupsModal` that calls `groups.refetch()` whenever `visible` transitions to `true`, so opening the modal always shows current server state regardless of how stale the cached value might be — this directly eliminates the "shows nothing even though it's in use elsewhere" failure mode, whatever its exact trigger, because the modal can no longer hold data older than "as of the moment it was opened."
- Update `crates/vexboard-frontend/src/pages/dashboard/modals.rs` to pass `groups=groups` into `<GroupsModal>`.
- Remove `GroupEntry` and `fetch_groups_internal` (dead after the switch).

## Implementation Steps

1. `crates/vexboard-server/src/db/models.rs`: change `UpdateQuickLink.group_id` from `Option<i64>` to `Option<Option<i64>>` with `#[serde(default, deserialize_with = "deserialize_some")]` + `#[schema(value_type = Option<i64>)]`, matching `UpdateService.group_id`'s existing attributes exactly.
2. `crates/vexboard-server/src/api/quick_links.rs:148`: change `payload.group_id.or(existing.group_id)` to `payload.group_id.unwrap_or(existing.group_id)`.
3. `crates/vexboard-frontend/src/pages/dashboard/mod.rs`: add `pub sort_order: i64` to `GroupResponse`; update `resolve_groups` — no change needed there (it doesn't read `sort_order`).
4. `crates/vexboard-frontend/src/components/modal_groups.rs`: remove `GroupEntry` struct and `fetch_groups_internal`; change `GroupsModal`'s signature to take `#[prop(into)] groups: LocalResource<Vec<GroupResponse>>` (or plain typed prop, matching how `services`/`quick_links` are passed elsewhere in this codebase — no `#[prop(into)]` needed since it's the same concrete type); replace internal `let groups = LocalResource::new(fetch_groups_internal);` with the passed-in prop; update all field reads (`g.color`, `g.name`, `g.id`, `g.sort_order`) to use `GroupResponse`'s fields (drop the unused `icon` read since `GroupResponse` doesn't carry it and it was already dead code); add an `Effect::new` that calls `groups.refetch()` when `visible.get()` becomes `true`.
5. `crates/vexboard-frontend/src/pages/dashboard/modals.rs`: pass `groups=groups` to `<GroupsModal>`.
6. Import `use crate::pages::dashboard::GroupResponse` (or wherever it's re-exported/visible from — check current `pub(super)` visibility; `GroupResponse` is currently `pub(super)` in `dashboard/mod.rs`, scoped to the `dashboard` module tree, and `modal_groups.rs` lives under `components/`, outside that tree — this visibility will need to be widened to `pub(crate)` for `GroupResponse` since `modal_groups.rs` needs to name the type in its function signature).

## Dependencies

None new. No Context7 lookup needed — internal Rust/Leptos/Axum change only, no new external library.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Widening `GroupResponse` visibility from `pub(super)` to `pub(crate)` could be flagged as unnecessary broadening.
  **Mitigation:** It's the minimum visibility needed for `components/modal_groups.rs` to reference the type; no `pub` (crate-external) exposure is introduced.
- **Risk:** Refetching on every modal open adds one extra network round-trip per open.
  **Mitigation:** Negligible cost (`GET /api/v1/groups` is already called elsewhere routinely); correctness here matters more than saving one small request.
- **Risk:** Removing `GroupEntry`/`fetch_groups_internal` could break something unexpectedly if referenced elsewhere.
  **Mitigation:** Both are private to `modal_groups.rs` (confirmed via grep — no other file references `GroupEntry` or `fetch_groups_internal`).

## Test Plan

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings` (server crate only, per project constraints — frontend crate is WASM-only and not compiled by this command; frontend changes are verified by `cargo clippy`'s failure to compile the workspace's native-target members if a shared type/visibility mistake is made, but the WASM crate itself must be checked separately if the developer has Trunk installed — out of scope for this preflight run per FORBIDDEN COMMANDS)
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
- Manual verification (frontend, cannot be exercised by the backend-only test suite): after rebuild, edit a quick link that has a group assigned, select "— No group —", save, and confirm it moves to "Ungrouped" in Group sort mode and stays there after a page refresh; open "Manage Groups" and confirm all existing groups are listed.
