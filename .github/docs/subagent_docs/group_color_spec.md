# Group Badge Color — Spec

## Current State

The `groups` table has no color column. In `dashboard.rs`, the Group sort mode hardcodes
`var(--color-accent)` / `var(--color-accent-dim)` / `rgba(59,130,246,0.3)` for every badge.
The `modal_groups.rs` create/edit form has no color picker.

## Problem

All group badges look identical (blue). Users cannot visually distinguish groups at a glance.

## Proposed Solution

1. Add a nullable `color TEXT` column to the `groups` table (hex string, e.g. `#3b82f6`).
2. Expose it in the `Group` model, `CreateGroup` and `UpdateGroup` DTOs, and all API queries.
3. In the Group sort view, derive badge `bg = color + "22"` and `border = color + "50"` from
   the stored hex value. Fall back to accent CSS vars when `color` is `None`.
4. Add a 9-swatch color palette picker to the Groups modal (create row + inline edit).

## Palette

| Name    | Hex       |
|---------|-----------|
| Blue    | #3b82f6   |
| Purple  | #8b5cf6   |
| Green   | #22c55e   |
| Orange  | #f97316   |
| Red     | #ef4444   |
| Pink    | #ec4899   |
| Yellow  | #eab308   |
| Teal    | #14b8a6   |
| Gray    | #6b7280   |

Default for new groups: `#3b82f6` (blue).

## Files Modified

- `crates/vexboard-server/src/db/migrations/004_group_color.sql` (new)
- `crates/vexboard-server/src/db/mod.rs`
- `crates/vexboard-server/src/db/models.rs`
- `crates/vexboard-server/src/api/groups.rs`
- `crates/vexboard-frontend/src/pages/dashboard.rs`
- `crates/vexboard-frontend/src/components/modal_groups.rs`

## Build/Validation Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo build --release --bin vexboard-server`

No new external dependencies — internal change only.
