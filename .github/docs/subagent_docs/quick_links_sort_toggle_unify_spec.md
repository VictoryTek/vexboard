# Quick Links Sort Toggle Unification — Spec

## Current State Analysis

A recent change ("Manage Quick Link Groups" feature) introduced quick link groups as an
independent concept from service groups (separate backend endpoint
`/api/v1/quick-link-groups`, separate table, separate modal
`components/modal_quick_link_groups.rs`). This separation is correct and intentional.

However, the same change also gave the Quick Links section its own, separate A-Z/Group
sort toggle, duplicating UI and state that already exists for Services:

- `crates/vexboard-frontend/src/pages/dashboard/mod.rs`
  - `SortMode` enum (lines 16-21): shared type, reused correctly.
  - Services sort signal (line 117): `let (sort_mode, set_sort_mode) = signal(SortMode::AZ);`
  - Services toggle UI (lines 152-178), writes via `set_sort_mode`.
  - Quick-links-specific sort signal (line 118):
    `let (ql_sort_mode, set_ql_sort_mode) = signal(SortMode::AZ);`
  - Passed to `<QuickLinksSection sort_mode=ql_sort_mode set_sort_mode=set_ql_sort_mode .../>`
    (lines 345-346).
- `crates/vexboard-frontend/src/pages/dashboard/quick_links_section.rs`
  - Duplicated AZ/Group toggle UI (lines 82-106), near-identical markup to the services toggle.
  - Component takes `sort_mode: ReadSignal<SortMode>` and `set_sort_mode: WriteSignal<SortMode>`
    (lines 17-18).
  - Branches on `sort_mode.get() == SortMode::Group` (line 110) to pick grouped vs. flat
    A-Z rendering.

Neither signal is persisted (no localStorage/settings) — both reset to `SortMode::AZ` on reload.

## Problem Definition

This was a miscommunication. The user wants:
- Quick link groups to remain independent of service groups (already true — no change needed).
- Only ONE A-Z/Group sort toggle in the UI, shared by both Services and Quick Links —
  not two separate toggles that could be set independently.

## Proposed Solution

Remove the quick-links-specific sort signal and its duplicated toggle UI. Have the
Quick Links section read the existing shared `sort_mode` signal (the one already driving
the Services toggle) instead of its own.

Note: the existing services toggle also has a `SortMode::Source` option that quick links
never used — this is fine, since quick links will simply treat `Source` as it currently
treats any non-`Group` value (falls into the flat/A-Z-sorted `else` branch at line 278).
No behavior change needed there.

## Implementation Steps

1. `crates/vexboard-frontend/src/pages/dashboard/mod.rs`
   - Delete line 118 (`ql_sort_mode` / `set_ql_sort_mode` signal).
   - Change the `<QuickLinksSection .../>` invocation (lines 345-346) to pass the shared
     `sort_mode` read signal only (no setter needed since the section will no longer own
     a toggle).
2. `crates/vexboard-frontend/src/pages/dashboard/quick_links_section.rs`
   - Remove the duplicated toggle UI block (lines 82-106).
   - Change component signature (lines 17-18) to accept `sort_mode: ReadSignal<SortMode>`
     only; drop the `set_sort_mode` prop.
   - Leave the `sort_mode.get() == SortMode::Group` branch (line 110) and grouped/flat
     rendering logic unchanged.

## Dependencies

None — internal-only change, no new external libraries. Context7 lookup not required per
policy (internal code change, no new dependency).

## Configuration Changes

None.

## Risks and Mitigations

- Risk: `set_sort_mode` prop removal could leave dead imports/props elsewhere.
  Mitigation: grep for `ql_sort_mode` / `set_ql_sort_mode` / `QuickLinksSection` call sites
  after edit to confirm no leftover references (already confirmed via research: only one
  call site in `mod.rs`).
- Risk: Quick links previously only supported AZ/Group; sharing the toggle also exposes
  `Source` mode to quick links. Since `Source` is not special-cased in
  `quick_links_section.rs`, it falls through to the existing flat-list `else` branch —
  same rendering as `AZ`. No visual regression, just an unused-for-quick-links mode value.
