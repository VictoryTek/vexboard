# Phase 1 Spec: Split dashboard.rs into Sub-Components

**Feature:** dashboard_split  
**Date:** 2026-06-06  
**Audit Entry:** 2.3.3

---

## Current State

`crates/vexboard-frontend/src/pages/dashboard.rs` — 940 lines, three distinct
concerns mixed together:
- Modal management (5 modal instances, ~90 lines)
- Service grid with drag-to-reorder and three sort modes (~500 lines)
- Quick links section (~60 lines)
- Data types, async helpers, and page state (rest)

## Problem

The file will continue to grow with each feature addition. Three separate
concerns make it hard to find and reason about any one part.

## Proposed Solution

Convert `dashboard.rs` to a module directory and extract into three components:

```
pages/
  dashboard/
    mod.rs                  ← DashboardPage + types + async helpers
    modals.rs               ← DashboardModals
    service_grid.rs         ← ServiceGrid
    quick_links_section.rs  ← QuickLinksSection
```

### Signal / Resource sharing

All Leptos reactive primitives (`RwSignal`, `ReadSignal`, `LocalResource`) are
`Copy` in Leptos 0.8 and can be passed as component props without cloning.

The three show-modal `(ReadSignal, WriteSignal)` pairs are consolidated to
`RwSignal<bool>` — all modals use `#[prop(into)] visible: Signal<bool>`, so
`RwSignal<bool>` coerces automatically.

### `DashboardModals` props
```
services, quick_links, groups: LocalResource<Vec<_>>
show_modal, show_add_link_modal, show_groups_modal: RwSignal<bool>
edit_target: RwSignal<Option<(i64, EditFormData)>>
edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>>
```
Defines save callbacks internally (has full access to all needed signals).

### `ServiceGrid` props
```
services, groups: LocalResource<Vec<_>>
sort_mode: ReadSignal<SortMode>
drag_src_idx, drag_over_idx: RwSignal<Option<usize>>
section_drag_src, section_drag_over: RwSignal<Option<(String, usize)>>
edit_target: RwSignal<Option<(i64, EditFormData)>>
```
Derives `is_admin` independently from `CurrentUser` context.
Calls `super::fetch_services()` and `super::reorder_services()`.

### `QuickLinksSection` props
```
quick_links: LocalResource<Vec<QuickLinkResponse>>
edit_link_target: RwSignal<Option<(i64, QuickLinkFormData)>>
```
Derives `is_admin` independently from context.

### `DashboardPage` (mod.rs) after extraction
Owns all state signals and resources. Renders the page header (sort controls +
"+ Add" dropdown) inline — it is tightly coupled to sort_mode, show_add_menu,
and the modal show signals and is not worth a separate component at this size.
Mounts the three sub-components.

## Implementation Steps

1. Create `pages/dashboard/` directory
2. Write `pages/dashboard/mod.rs`
3. Write `pages/dashboard/modals.rs`
4. Write `pages/dashboard/service_grid.rs`
5. Write `pages/dashboard/quick_links_section.rs`
6. Delete `pages/dashboard.rs`

## Dependencies

None new. `pages/mod.rs` `pub mod dashboard` resolves to the directory automatically.

## Build / Test Commands

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `scripts/preflight.sh`

## Risks

Low. Logic is unchanged. Leptos signals and resources are Copy — no lifetime
issues. The view macro supports `use`-imported sub-module components normally.
