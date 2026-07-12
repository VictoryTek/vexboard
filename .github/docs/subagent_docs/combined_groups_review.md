# Combined Groups (Services + Quick Links) — Review

**Feature:** Unify service groups and quick-link groups into one group concept; grouped view shows a group's services in a row above its quick links.
**Date:** 2026-07-11
**Phase:** 3 — Review & Quality Assurance

---

## 1. Specification Compliance

Implemented per `combined_groups_spec.md` section 3:

- **3a (DB migration)**: `crates/vexboard-server/src/db/migrations/008_unify_groups.sql` + `unify_quick_link_groups()` in `db/mod.rs` copy every `quick_link_groups` row into `groups` (auto-suffixing on name collision, per the spec's stated default of "keep them distinct"), remap `quick_links.group_id` to the new ids, rebuild `quick_links` so its FK targets `groups`, and drop `quick_link_groups`. Runs inside a transaction, guarded by table-existence check for idempotency across restarts.
- **3b (Backend)**: `QuickLinkGroup`/`CreateQuickLinkGroup`/`UpdateQuickLinkGroup` models and the `api/quick_link_groups.rs` route module are removed; routes and OpenAPI schema/paths/tags updated to match.
- **3c (Frontend)**: New `group_section.rs` (`GroupSection` component) renders, per group, a services row above a quick-links row inside one shared container — used only in `SortMode::Group`. `ServiceGrid`/`QuickLinksSection` retain their `AZ`/`Source`/empty-state rendering unchanged and render nothing in Group mode (delegated to `GroupSection`). `modal_groups.rs` (`GroupsModal`) is unchanged and now serves as the single "Manage Groups" UI; the separate `modal_quick_link_groups.rs`/`QuickLinkGroupsModal` and its dropdown menu entry are removed.
- **3d (rollout order)**: followed as specified.

Both spec open questions were resolved with the stated defaults (collision → auto-suffix distinct groups; empty groups → hidden, unchanged from prior behavior, since `GroupSection` skips any section with zero services and zero quick links, matching the prior per-type behavior of skipping empty sections).

## 2. Best Practices / Consistency

- Migration follows the existing embedded-SQL + idempotency-guard pattern already used by migrations 003, 004, 006.
- Drag-and-drop and reset-to-A-Z logic in `GroupSection` is moved (not rewritten) from the removed `Group` branches of `service_grid.rs`/`quick_links_section.rs`, preserving exact sort/tiebreak semantics (`sort_order` then case-insensitive name).
- `live_status` (SSE-driven probe status) was lifted from `ServiceGrid` to `DashboardPage` so both `ServiceGrid` and `GroupSection` observe the same live state regardless of sort mode — necessary because Group mode no longer routes through `ServiceGrid`.

## 3. Functionality

- New group semantics: a group can now hold services only, quick links only, or both — verified via code path (single `groups` table, single FK target from both `services.group_id` and `quick_links.group_id`).
- "Ungrouped" bucket in Group mode now combines leftover services and leftover quick links.

## 4. Security

No new attack surface: no new endpoints; the removed `quick_link_groups` endpoints are simply deleted since their functionality is fully absorbed by the existing `/api/v1/groups` CRUD, which was already admin-gated.

## 5. Build Validation

All commands run were on the Phase 1 spec's approved/safe list:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `SQLX_OFFLINE=true cargo clippy --workspace -- -D warnings` | Pass (after fixing one `clippy::type_complexity` finding in the migration code, and one real SQL-syntax bug caught by testing — see below) |
| `SQLX_OFFLINE=true cargo test -p vexboard-server` | 34/34 pass |
| `SQLX_OFFLINE=true cargo build --release --bin vexboard-server` | Pass |
| `cd crates/vexboard-frontend && cargo check --target wasm32-unknown-unknown` | Pass (safe read-only type-check; not `trunk build`, which stays on the FORBIDDEN list since Trunk CLI presence wasn't confirmed) |

**Bug found and fixed during review**: the initial migration implementation updated `quick_links.group_id` in place before rebuilding the table, which violates SQLite's FK constraint (the old column still targets `quick_link_groups(id)`, but the new value is a `groups.id`). Fixed by computing the remapped `group_id` inline during the table-rebuild `INSERT...SELECT` instead, so the FK is only ever satisfied by values valid at write time. A second bug — a malformed `CASE ... ELSE ... END` expression with zero `WHEN` clauses whenever no `quick_link_groups` rows exist (the common case for installs that never created one) — was caught by running the full server test suite (13 tests failed with a SQL syntax error) and fixed by special-casing an empty id-map to a plain `NULL`. Both fixes were verified with temporary in-memory-SQLite tests exercising the collision-rename path, the plain remap path, and the empty-table path, then removed (not committed, per the "don't add speculative test coverage beyond what's asked" instruction) once confirmed correct.

## 6. Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 95% | A |
| Functionality | 100% | A |
| Code Quality | 92% | A- |
| Security | 100% | A |
| Performance | 95% | A |
| Consistency | 95% | A |
| Build Success | 100% | A |

**Overall Grade: A (97%)**

## Returns

- **Build result**: all approved commands pass.
- **Verdict: PASS** — proceeding to Phase 6 (Preflight).
