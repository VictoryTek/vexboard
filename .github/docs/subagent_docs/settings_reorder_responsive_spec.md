# Settings Page Reorder + Responsive Stacking — Specification

**Feature:** `settings_reorder_responsive`
**Date:** 2026-07-10
**Builds on:** `settings_ui_facelift` (previously implemented `.settings-grid` auto-fit layout)

---

## 1. Current State Analysis

`crates/vexboard-frontend/src/pages/settings.rs` renders five cards inside `.settings-grid`
in this DOM order: Appearance, Navigation Sidebar, Service Discovery, About,
User Management (admin-only, full width).

`crates/vexboard-frontend/style/main.css` (`.settings-grid`, line ~465):

```css
.settings-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
  gap: 1rem;
  align-items: start;
}
```

**Confirmed via `grep -n "@media" main.css`: there are zero media queries anywhere in
this stylesheet.** The prior facelift spec assumed `auto-fit` would "naturally collapse"
on narrow screens, but `auto-fit`/`minmax` only reflows column *count* — it does not
reorder cards, and with `minmax(340px, 1fr)` a viewport narrower than ~340px still
overflows rather than gracefully stacking. Card heights are also mismatched
(Navigation Sidebar has 3 stacked option rows; Appearance/Service Discovery/About are
one or two lines), which is the main source of the "messy" look on desktop reported
by the user, alongside the un-ordered discovery that About should be the lead card.

---

## 2. Problem Definition

1. **No responsive breakpoint exists.** Below some width the grid needs to collapse to
   a single column explicitly.
2. **Card order is arbitrary.** The user wants **About first**, in both desktop and
   mobile layouts (confirmed via user clarification — not a mobile-only reorder).
3. **Uneven card heights on desktop** make the current 4-up row look disjointed.

---

## 3. Proposed Solution Architecture

### 3.1 Reorder cards in `settings.rs`

Move the About card's JSX block to be the first child of `.settings-grid`, ahead of
Appearance. New DOM order: **About, Appearance, Navigation Sidebar, Service Discovery**,
then User Management (admin-only, full width, unchanged — it already visually anchors
the bottom via `grid-column: 1 / -1`).

No CSS `order` property needed since this is a real DOM reorder applied at all
breakpoints, matching the user's answer that About should lead everywhere, not just
under a mobile media query.

### 3.2 Add a mobile breakpoint

Add a single media query to `.settings-grid` in `main.css`:

```css
@media (max-width: 640px) {
  .settings-grid {
    grid-template-columns: 1fr;
  }
}
```

This collapses to a single stacked column below 640px (Tailwind's conventional `sm`
breakpoint, consistent with the project's existing 340px card minimum — two cards
can't fit side-by-side much below 680px anyway, so 640px cleanly forces the stack
before things get cramped). Because the DOM order already has About first (3.1), no
`order` overrides are required for the mobile case either.

### 3.3 Desktop visual balance (uneven heights)

Set `align-items: stretch` instead of `start` only for the four small/medium cards
row, OR simpler: leave `align-items: start` (existing) but this is cosmetic and not
explicitly requested beyond "looks terrible" which the user's answers scoped down to
reorder + mobile stacking. **No additional height-balancing change will be made** —
out of scope per the user's clarification, which focused the ask on ordering and
mobile stacking, not a full visual redesign. If height mismatch still reads as messy
after this change, that's a separate follow-up.

---

## 4. Implementation Steps

1. In `settings.rs`, cut the "About" `<div class="card">...</div>` block and its
   preceding comment, and paste it as the first child of `.settings-grid`, before the
   "Appearance" card.
2. In `main.css`, add the `@media (max-width: 640px)` rule directly after the
   `.settings-grid` block (~line 470).
3. No Rust logic, signals, or props change — this is a pure JSX-block-reordering +
   CSS change.

---

## 5. Files to Modify

| File | Change |
|---|---|
| `crates/vexboard-frontend/src/pages/settings.rs` | Reorder About card to first position in `.settings-grid` |
| `crates/vexboard-frontend/style/main.css` | Add `@media (max-width: 640px)` single-column rule for `.settings-grid` |

---

## 6. Dependencies

None. No new external libraries. Context7 lookup not required (pure internal
CSS/Leptos view markup change).

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
confirmed installed on this machine (FORBIDDEN COMMANDS policy) — visual verification
of the reorder/breakpoint will need to happen via manual `trunk serve` by the user, or
a follow-up session where the toolchain is confirmed present.

---

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Reordering the About block breaks a `key`/`id`-dependent list rendering | No `<For>` or keyed list is involved for these four static cards — plain JSX reordering is safe |
| 640px breakpoint conflicts with some other not-yet-discovered responsive rule | Confirmed via grep that no other media queries exist in `main.css`; this is the first and only one touching `.settings-grid` |
| Frontend crate can't be compiled/tested natively to verify the JSX change | Rely on `cargo clippy --workspace` (compiles server crate; frontend crate is WASM-only and excluded per project constraints) plus manual code read-through; flag to user that visual confirmation requires `trunk serve` |
