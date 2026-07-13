# Group Alphabetical Sort — Spec

## Current State

`SortMode::Group` display is rendered by `crates/vexboard-frontend/src/pages/dashboard/group_section.rs`.

- `group_list` (a `Vec<GroupResponse>`) is fetched via `/api/v1/groups`, which the backend
  (`crates/vexboard-server/src/api/groups.rs:37-39`) returns ordered by `sort_order ASC`
  (manual/creation order used for group-management UI, not alphabetical).
- `sections_data` (lines 134-158) is built by iterating `group_list` in that same
  `sort_order` order — no sort by group name is ever applied to `sections_data` itself.
- Items *within* each group (services, quick links) already sort alphabetically
  case-insensitively (lines 145-148, 169-172), so only the group-level ordering is
  affected by this bug.
- The synthetic `"Ungrouped"` section is pushed onto the end of `sections_data` at line
  173, after real groups, and must remain last regardless of alphabetical order.

## Problem

When sorted by Group, sections appear in group `sort_order` (creation/manual-reorder
order) instead of alphabetical order by group name.

## Solution

In `group_section.rs`, immediately after building `sections_data` via `.collect()`
(end of line 158) and before the `ungrouped_svcs`/`ungrouped_links` block (line 160),
insert a case-insensitive alphabetical sort on the group name field (`.1` of the
`Section` tuple):

```rust
sections_data.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
```

Because "Ungrouped" is pushed onto `sections_data` *after* this sort runs, it is
unaffected and continues to render last unconditionally — no special-casing needed.

## Implementation Steps

1. Edit `crates/vexboard-frontend/src/pages/dashboard/group_section.rs`: add the
   one-line `sort_by` call described above.
2. No other files require changes. No new dependencies. No backend or config changes.

## Dependencies

None (no new external libraries; Context7 lookup not required — pure internal Rust/Leptos
sort logic).

## Risks and Mitigations

- **Risk:** Sorting could accidentally include "Ungrouped" if the sort is placed after
  its push. **Mitigation:** sort call is placed before the push (line 160 vs 173).
- **Risk:** None to backend `sort_order` field — it remains used elsewhere (e.g. group
  management modal) unaffected by this change, since this only reorders the local
  `sections_data` vec for `SortMode::Group` rendering.
