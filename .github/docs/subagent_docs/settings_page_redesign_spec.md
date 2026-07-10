# Settings Page Redesign (Concept A: Sectioned List) — Specification

**Feature:** `settings_page_redesign`
**Date:** 2026-07-10
**Supersedes:** `settings_reorder_responsive_spec.md` (narrower reorder-only spec, superseded
after the user asked for a fuller redesign and approved a mockup — see below)

---

## 1. Current State Analysis

`crates/vexboard-frontend/src/pages/settings.rs` renders a `.settings-grid` CSS grid
(`grid-template-columns: repeat(auto-fit, minmax(340px, 1fr))`) containing five
independent `.card` tiles: Appearance, Navigation Sidebar, Service Discovery, About,
User Management (admin-only, `.settings-card-full`).

Confirmed problems (via direct code read + user screenshot):

1. **No responsive breakpoint anywhere in `main.css`** (`grep -n "@media"` returns
   nothing) — the grid doesn't stack cleanly on narrow viewports.
2. **Mismatched card heights** — Navigation Sidebar (3 stacked options) is much taller
   than Appearance/Service Discovery/About (one or two lines), producing a visually
   disjointed row.
3. **`class="input"` on the Add User form fields (lines ~307, 321, 331 of
   `settings.rs`) references a CSS class that does not exist anywhere in
   `main.css`.** These inputs render with unstyled browser defaults (stark white
   boxes), which is the glaring visual defect visible in the reported screenshot.
   `main.css` already defines an equivalent `.form-input` class (line 377) used
   elsewhere in the app that is never applied here.
4. Card order is arbitrary; user wants About treated as a lead identity strip, not a
   tile competing for grid space.

A user-approved mockup (Concept A, "Sectioned list") replaces the tiled grid with a
single-column list of label-left/control-right rows inside one bordered container,
plus a slim About banner at the top — matching patterns like GitHub/Linear settings
pages. This is the shape to implement.

---

## 2. Problem Definition

Redesign the settings page layout and styling:
- Fix the broken `.input` styling (unstyled white inputs).
- Replace the uneven multi-column tile grid with a single-column sectioned list.
- Give About a lead position as a compact top banner, not a competing tile.
- Add a mobile breakpoint so long label/control rows stack cleanly on narrow screens.

---

## 3. Proposed Solution Architecture

### 3.1 Structure (`settings.rs`)

Replace `<div class="settings-grid">...</div>` with:

```
<div class="settings-about-banner"> ... info-circle icon + "VexBoard v{version} — ..." ... </div>

<div class="settings-list">
  <div class="settings-row"> Appearance:        label (icon+title+desc) | control (toggle button) </div>
  <div class="settings-row"> Navigation Sidebar: label (icon+title+desc) | control (3 option pills) </div>
  <div class="settings-row"> Service Discovery:  label (icon+title+desc) | control (descriptive paragraph) </div>
  <Show when=is_admin>
    <div class="settings-row"> User Management:  label (icon+title+desc) | control (user table + add-user form) </div>
  </Show>
</div>
```

- `.settings-row` is `display:flex` with a fixed-width label column (`.settings-row-label`,
  `flex: 0 0 240px`) and a flexible control column (`.settings-row-control`, `flex: 1`).
- Each row has a `border-bottom` divider except the last (`:last-child` removes it).
- The whole `.settings-list` is one bordered/rounded container (reusing the existing
  `.card`-like surface treatment) so the page reads as one cohesive block instead of
  five separate tiles.
- Existing `.settings-section-header` / `.settings-section-icon` classes are reused for
  the `<h3>` inside each `.settings-row-label` (icon + title). Existing
  `.settings-nav-option` / `.settings-nav-option-active` / `.settings-nav-dot` classes
  are kept as-is for the sidebar-mode control, just now rendered horizontally
  (`display:flex; gap` container) instead of stacked, matching the "option pills" in
  the approved mockup.
- `.settings-card-full` and `.settings-grid` become unused after this change and are
  removed from `main.css` (no other file references them, confirmed via grep).

### 3.2 Fix the `.input` bug

Change `class="input"` → `class="form-input"` on the three Add User fields
(username input, password input, role select) in `settings.rs`. `.form-input` already
exists in `main.css` (line 377) and is used elsewhere in the app, so this is a
one-line-per-field class rename, not a new style.

### 3.3 New CSS (`main.css`)

Added directly after the current `.settings-grid` block (replacing it):

```css
.settings-about-banner {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  background-color: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: 10px;
  padding: 0.875rem 1.125rem;
  margin-bottom: 1.5rem;
}

.settings-about-banner .settings-section-icon {
  color: var(--color-text-secondary);
}

.settings-about-text {
  font-size: 0.8rem;
  color: var(--color-text-secondary);
}

.settings-about-text strong {
  color: var(--color-text-primary);
  font-weight: 600;
}

.settings-list {
  background-color: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: 12px;
  padding: 0 1.5rem;
}

.settings-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 2rem;
  padding: 1.5rem 0;
  border-bottom: 1px solid var(--color-border);
}

.settings-row:last-child { border-bottom: none; }

.settings-row-label { flex: 0 0 240px; }

.settings-row-control { flex: 1; min-width: 0; }

.settings-option-row { display: flex; gap: 0.6rem; flex-wrap: wrap; }

@media (max-width: 720px) {
  .settings-row { flex-direction: column; gap: 0.75rem; }
  .settings-row-label { flex: none; }
}
```

`.settings-section-header` gets its `margin-bottom` reduced from `0.875rem` to
`0.3rem` since it's now a label heading sitting directly above a one-line
description, not a card header sitting above a taller body.

`.settings-nav-option` / `.settings-nav-option-active` are wrapped in a new
`.settings-option-row` flex container (replacing the old `.space-y-2` vertical stack)
so the three options lay out horizontally per the approved mockup, wrapping onto a
second line on narrow screens for free via `flex-wrap: wrap`.

---

## 4. Implementation Steps

1. `main.css`: replace the `.settings-grid` / `.settings-card-full` block with the
   `.settings-about-banner` / `.settings-list` / `.settings-row` / `.settings-option-row`
   block above; add the `@media (max-width: 720px)` rule; reduce
   `.settings-section-header` margin-bottom.
2. `settings.rs`: restructure the view body per §3.1 — About banner first, then
   `.settings-list` containing Appearance / Navigation Sidebar / Service Discovery /
   User Management rows. Preserve all existing signals, event handlers, and logic
   (theme toggle, sidebar mode selection, user CRUD) untouched — only the surrounding
   markup/classes change.
3. `settings.rs`: rename `class="input"` to `class="form-input"` on the three Add User
   form fields.

---

## 5. Files to Modify

| File | Change |
|---|---|
| `crates/vexboard-frontend/src/pages/settings.rs` | Restructure to sectioned-list layout; fix `.input` → `.form-input` |
| `crates/vexboard-frontend/style/main.css` | Replace `.settings-grid` styles with sectioned-list styles + mobile breakpoint |

---

## 6. Dependencies

None. No new external libraries; pure Leptos view markup + CSS. Context7 lookup not
required.

---

## 7. Configuration Changes

None.

---

## 8. Build / Test Commands (Phase 3)

Per CLAUDE.md approved safe commands:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`

WASM frontend (`trunk build`) is not run unless Trunk + `wasm32-unknown-unknown` are
confirmed installed (FORBIDDEN COMMANDS policy). Visual confirmation of the new
layout requires the user to run `trunk serve` (or equivalent) themselves, or a
follow-up session where the toolchain is confirmed present.

---

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Removing `.settings-grid`/`.settings-card-full` breaks something else | Confirmed via grep these classes are only referenced in `settings.rs`, which is being updated in the same change |
| Renaming `.input` → `.form-input` changes visual sizing unexpectedly | `.form-input` sets `width: 100%`, which still fills the flex item exactly as the ad-hoc unstyled input did; padding/border/radius are added, which is the intended fix |
| `flex: 0 0 240px` label column feels cramped for the longer descriptions | Verified against real copy (longest is the Service Discovery description) during implementation; column width can be adjusted to 260-280px if control column looks starved, still within spec intent |
| Frontend crate can't be compiled natively to verify the JSX change | Rely on `cargo clippy --workspace` (server crate only, per project constraints) plus careful manual read-through; visual confirmation flagged to user as needing `trunk serve` |
