# Mobile Responsive Layout — Spec

## Current State Analysis

- `crates/vexboard-frontend/index.html` mounts the Leptos app directly into `<body>` via
  `mount_to_body` (`src/main.rs:24`) with no placeholder content. `body` background is
  `--color-bg-primary: #0a0c10` (near-black). If the WASM bundle is slow to fetch/instantiate
  or fails outright (much more likely on a mobile network/browser than desktop), the user sees
  nothing but a dark rectangle — no spinner, no error, no timeout message.
- `style/main.css` (615 lines) contains exactly one `@media` rule, scoped to the Settings page
  (`@media (max-width: 720px)` at line 521). The sidebar, metric bar, and service grid have
  zero responsive behavior:
  - `.sidebar` (`main.css:73`) is a vertical flex column, width driven by an **inline style**
    in `src/components/sidebar.rs:63` (`format!("width: {}px", ...)` — 220px expanded / 60px
    collapsed). Expansion is hover-driven (`on:mouseenter`/`on:mouseleave`), which never fires
    on touch — so on a phone the sidebar is always stuck at 60px collapsed, permanently taking
    up horizontal space with no way to see nav labels.
  - `.metric-bar` (`main.css:163`) is a single non-wrapping flex row (CPU/RAM/IN/OUT/DISK +
    user menu) with `overflow: visible`. On a ~375px-wide phone viewport it doesn't fit and
    items are simply clipped/pushed off with no scroll affordance.
  - The service/quick-link grids use `grid-template-columns: repeat(auto-fill, minmax(320px,1fr))`
    (`src/pages/dashboard/service_grid.rs:40,261,332` and similarly in
    `quick_links_section.rs`, `group_section.rs`). A 320px minimum column doesn't fit in the
    space left after the sidebar + `1.5rem` content padding on any phone in portrait.
  - `MainLayout` (`src/main.rs:104-114`) lays out sidebar + main content with **inline styles**
    (`display:flex; height:100vh; overflow:hidden;`), not CSS classes, so there is currently no
    hook for a `@media` rule to retarget this layout for narrow viewports at all.
- Chrome DevTools "device toolbar" only emulates viewport width (which is what CSS `@media`
  queries key off) — it does not emulate hover. So the reported "devtools does not convert"
  symptom is explained entirely by the missing `@media` rules; no touch-specific JS is needed
  to fix that part.
- No service worker, no PWA manifest, no CSP headers found (`crates/vexboard-frontend/public/`,
  grep of `crates/vexboard-server/src` for CSP) — ruled out as contributing causes.
- `EventSource` (SSE, used in `metric_bar.rs` and `dashboard/mod.rs`) is supported in all modern
  mobile browsers — ruled out as a crash source.

## Problem Definition

1. No responsive/mobile layout exists — confirmed via CSS audit above.
2. No loading feedback while WASM boots — on a slow/flaky mobile connection this presents as a
   dark, blank, apparently-frozen page, matching the user's "screen just goes dark and nothing
   loads" report.

## Proposed Solution

### A. Boot-time loading placeholder (`index.html` + `main.rs`)
- Add a `#initial-loader` div directly in `index.html`'s `<body>`, before the Trunk-injected
  wasm script tag, with a centered spinner + "Loading VexBoard…" text styled inline (no CSS
  framework dependency, since it must render before `main.css`'s Tailwind layer is guaranteed
  parsed... actually `main.css` is a `<link>` in `<head>`, so it *is* available — style it via
  existing CSS vars for consistency instead of hardcoding colors).
- Add a small inline `<script>` timeout (e.g. 8s) that appends a secondary "still loading /
  check your connection" line to the loader — pure vanilla JS, no new dependency, gives a
  mobile user (no devtools) a concrete signal instead of silence.
- In `src/main.rs::main()`, remove the `#initial-loader` element (via `web_sys`) as the very
  first statement, before `mount_to_body`. This only runs once WASM has actually started
  executing, so the loader's presence/absence is a true signal of WASM boot success.

### B. Responsive CSS breakpoint at `max-width: 768px` (Tailwind's `md` breakpoint, consistent
   with the existing Settings page breakpoint style)
- Convert the inline-styled containers in `MainLayout` (`main.rs:104-114`) to CSS classes
  (`.app-shell`, `.app-main`, `.app-content`) so a `@media` rule can retarget them. Behavior at
  desktop widths is unchanged (same flex values, just moved from inline `style` to CSS).
- Sidebar (`.sidebar` in `main.css`, mobile override only):
  - `width: 100% !important` (overrides the per-instance inline px width from `sidebar.rs`),
    fixed to the bottom of the viewport, `flex-direction: row`, no hover-expand text (already
    naturally suppressed since `is_expanded()` is driven by `hovered`, which never becomes
    `true` without a `mouseenter` event — no Rust/component change needed here).
  - Hide `.sidebar-logo` on mobile (no room in a bottom bar).
  - `.app-content` gets bottom padding equal to the mobile sidebar's height so content isn't
    hidden behind the fixed bottom bar.
- Metric bar (`.metric-bar`, mobile override): `overflow-x: auto; overflow-y: hidden;` so the
  existing items become horizontally scrollable instead of clipped/overflowing invisibly.
- Service/quick-link grids: add `@media (max-width: 640px)` narrowing
  `grid-template-columns` from `minmax(320px,1fr)` to `minmax(140px,1fr)` for the three grid
  sites in `service_grid.rs`, plus the equivalent grids in `quick_links_section.rs` and
  `group_section.rs` (to be located during implementation — same `minmax(320px,1fr)` pattern
  expected).

## Implementation Steps

1. `index.html` — add `#initial-loader` markup + inline timeout script.
2. `src/main.rs` — remove loader element at top of `main()`; convert `MainLayout`'s inline
   container styles to CSS classes (`app-shell`, `app-main`, `app-content`).
3. `style/main.css` — add the new `.app-shell`/`.app-main`/`.app-content` base rules (desktop
   values matching current inline styles exactly) + the `max-width: 768px` and
   `max-width: 640px` mobile overrides described above.
4. `src/components/sidebar.rs` — no logic change expected; verify no inline style conflicts
   with the new mobile CSS (the width inline style needs the CSS override to carry `!important`
   to win, per the CSS cascade: an external `!important` rule beats a plain inline style).
5. Grep for all `grid-template-columns:repeat(auto-fill,minmax(320px,1fr))` occurrences across
   `src/pages/dashboard/` and add the mobile grid override for each.

## Dependencies

None — no new crates or JS libraries. Context7 lookup not required (no external library/API
being added; Tailwind/Leptos usage patterns are unchanged, only vanilla CSS `@media` queries and
existing `web_sys` APIs already used elsewhere in the crate).

## Configuration Changes

None.

## Risks and Mitigations

- **Cannot compile-check the frontend crate locally**: Trunk CLI and the `wasm32-unknown-unknown`
  target are not installed on this machine (verified: `rustup target list --installed` has no
  wasm32 entry, no `trunk` on PATH). Per CLAUDE.md FORBIDDEN COMMANDS, `trunk build`/`trunk serve`
  must not be run without confirming both are present. **Mitigation**: validation for this change
  is limited to `cargo fmt --all -- --check` (syntax-safe, no compilation) plus careful manual
  review of Rust/`view!` macro syntax and CSS. This limitation will be reported explicitly to the
  user in the final delivery — the change should be verified with a real `trunk build` /
  on-device test before being considered fully confirmed.
- **CSS specificity for the sidebar width override**: inline `style` attributes normally beat
  external stylesheet rules; using `!important` in the mobile media query is required and is the
  standard, minimal way to solve this — documented so the reviewer doesn't flag it as an
  anti-pattern without context.
- **Grid breakpoint values (140px/640px) are a judgment call**, not from a design spec — chosen
  to fit 2 columns comfortably on a ~375-414px phone viewport after padding. Flagged as a
  reasonable default, adjustable later if the user wants a different density.
