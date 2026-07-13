# Responsive Grid Columns — Spec

## Current State Analysis

Both the service-card grid and the quick-links grid are Leptos inline-style CSS grids (no dedicated CSS classes, no Rust-side chunking/pagination — confirmed via `grep -rn "chunks(" crates/vexboard-frontend/src/` returning no results).

**Service cards** — `grid-template-columns: repeat(auto-fill, minmax(320px, 360px))`, no `max-width` anywhere in the ancestor chain (sidebar → `<main>` → padded content div → grid). Occurrences:
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs:34` (loading skeleton)
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs:254` (Source-mode sections)
- `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs:325` (default/A-Z mode)
- `crates/vexboard-frontend/src/pages/dashboard/group_section.rs:265` (Group mode)

Because the `minmax()` upper bound is fixed at `360px`, columns never grow to fill leftover row width — the grid always wraps as soon as a 360px+16px-gap track no longer fits, leaving dead space on the right. On a typical 1920px viewport with the sidebar expanded, this arithmetic lands at exactly 4 columns.

**Quick links** — `grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); max-width: 1200px`. Occurrences:
- `crates/vexboard-frontend/src/pages/dashboard/quick_links_section.rs:149`
- `crates/vexboard-frontend/src/pages/dashboard/group_section.rs:350`

Here `1fr` already lets columns flex to fill the row, but the explicit `max-width: 1200px` caps the row at 5 columns of `minmax(200px, 1fr)` regardless of actual available viewport width — a real, fixed constraint independent of screen size.

## Problem Definition

Neither grid dynamically uses all available row width on larger screens: service cards stop growing at a fixed 360px card width (wasting space instead of adding more columns or widening cards), and quick links are hard-capped at 5 columns by a fixed 1200px `max-width` even on much wider viewports.

## Proposed Solution

Pure CSS changes, no Rust logic changes, no new dependencies:

1. **Service cards**: change the `minmax()` upper bound from a fixed `360px` to `1fr` so columns flex to consume all available row width, fitting as many 320px-minimum cards as the container allows before wrapping. New value: `repeat(auto-fill, minmax(320px, 1fr))`.
2. **Quick links**: remove the fixed `max-width: 1200px` so the grid uses the full available container width; keep `minmax(200px, 1fr)` so link tiles keep flexing and more columns appear on wider screens. New value: `repeat(auto-fill, minmax(200px, 1fr)); ` (drop `max-width`).

Both grids already use `auto-fill`, so no other rules need to change — this only touches the `minmax()` bound and drops the one fixed `max-width`.

## Implementation Steps

1. In `service_grid.rs` (lines 34, 254, 325) and `group_section.rs:265`, replace `minmax(320px,360px)` with `minmax(320px,1fr)`.
2. In `quick_links_section.rs:149` and `group_section.rs:350`, replace `minmax(200px,1fr); gap:0.75rem; max-width:1200px;` with `minmax(200px,1fr); gap:0.75rem;` (drop `max-width:1200px`).
3. No Rust logic, component signatures, or props change — this is a pure inline-style string edit in each of the 6 call sites.

## Dependencies

None. No new libraries; Context7 lookup not applicable (CSS-only change).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk**: Removing `max-width` on quick links could make tiles stretch very wide on ultra-wide monitors since `1fr` divides remaining space among existing columns until a new column fits.
  - **Mitigation**: This is consistent with the existing service-card behavior after this change and is the explicitly requested "fit as many as it can" behavior; acceptable per user request.
- **Risk**: None on smaller viewports — `auto-fill` + `minmax()` degrades gracefully down to 1 column, unaffected by these changes.
- **Verification**: Visual check via `trunk serve` is the ideal validation but is gated by FORBIDDEN COMMANDS unless Trunk + `wasm32-unknown-unknown` are confirmed installed. Otherwise this is validated via `cargo fmt`, `cargo clippy`, and code review of the exact CSS string diffs (string-literal changes, no compile-affecting logic).
