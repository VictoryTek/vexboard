# Settings UI Facelift + Avatar Customization — Specification

**Feature:** `settings_ui_facelift`
**Date:** 2026-06-06

---

## 1. Current State Analysis

### settings.rs
- All cards are constrained to `max-width: 540px; margin: 0` — hard-left on any screen wider than 540px
- Section headers are plain text with no iconography
- Navigation sidebar mode buttons use `nav-item` / `nav-item-active` classes but with inline overrides that look inconsistent
- Overall visual density is sparse but not polished; lacks the same depth/layering as service cards on the dashboard

### user_menu.rs
- Account Settings modal has password/username fields but **no avatar customization**
- The avatar colour is hardcoded as `background: #4a90d9` in `.user-menu-avatar` CSS
- No mechanism to persist or apply a user-chosen colour

### main.css
- `.user-menu-avatar` background is a hardcoded hex — not driven by a CSS variable or localStorage

---

## 2. Problem Definition

1. **Layout**: Settings page does not use available horizontal space. On a 1440px display, all cards sit in a narrow 540px column at the left edge.
2. **Visual blandness**: Cards lack section icons and the styling lags behind the polished service cards / dashboard widgets.
3. **Missing feature**: Avatar customisation was previously present in Account Settings and is now absent.

---

## 3. Proposed Solution Architecture

### 3.1 Settings Page Layout

Replace the single `max-width: 540px` wrapper with a **CSS Grid** layout:

```
grid-template-columns: repeat(auto-fit, minmax(340px, 1fr))
```

- Appearance card and Navigation sidebar card sit side-by-side on wide screens
- Service Discovery and About cards also tile horizontally
- User Management (admin-only) spans full width because of the table + form
- On narrow screens (< ~700px) everything collapses to a single column

### 3.2 Section Icon Headers

Each card's `<h2>` gains a leading inline SVG icon (16×16, `currentColor`):

| Section | Icon |
|---|---|
| Appearance | sun/moon path |
| Navigation Sidebar | sidebar/layout path |
| Service Discovery | radar/search path |
| User Management | users path |
| About | info-circle path |

Icons use `display: inline-flex; align-items: center; gap: 0.5rem` so the text sits next to the icon cleanly.

### 3.3 Navigation Mode Buttons

The three sidebar mode options are restyled as visually distinct "option cards":
- Border, padding, hover highlight matching the rest of the design system
- Active state: `var(--color-accent-dim)` background + `var(--color-accent)` border + coloured icon dot
- Inactive state: `var(--color-bg-elevated)` background + `var(--color-border)` border

### 3.4 Avatar Customisation

**Storage:** `localStorage` key `vexboard-avatar-color` — a hex string e.g. `#3b82f6`.

**UI placement:** Account Settings modal (`user_menu.rs`), above the current password field.

**Colour swatches:** 8 predefined colours matching the design system palette:

| Colour | Hex |
|---|---|
| Blue (default) | `#3b82f6` |
| Indigo | `#6366f1` |
| Purple | `#a855f7` |
| Pink | `#ec4899` |
| Emerald | `#10b981` |
| Amber | `#f59e0b` |
| Red | `#ef4444` |
| Slate | `#64748b` |

**Implementation:**
1. `user_menu.rs`: Add `(avatar_color, set_avatar_color)` signal, initialised from `localStorage` via `#[cfg(target_arch = "wasm32")]`
2. In the Account Settings modal, render 8 swatch `<button>` elements in a flex row; clicking one calls `set_avatar_color` and writes to `localStorage`
3. Avatar display: pass `avatar_color` signal into the `style` attribute of `.user-menu-avatar` so it reads `background: <color>`
4. On save/close the colour is already persisted — no server round-trip needed

### 3.5 CSS Additions

New classes added to `main.css`:

```css
.settings-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 1rem; }
.settings-card-full { grid-column: 1 / -1; }
.settings-section-header { display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem; font-weight: 600; color: var(--color-text-primary); margin-bottom: 0.75rem; }
.settings-section-icon { width: 16px; height: 16px; flex-shrink: 0; color: var(--color-accent); }
.settings-nav-option { ... } /* see details in implementation */
.settings-nav-option-active { ... }
.avatar-swatch-row { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 1rem; }
.avatar-swatch { width: 28px; height: 28px; border-radius: 50%; border: 2px solid transparent; cursor: pointer; transition: transform 100ms, border-color 150ms; }
.avatar-swatch:hover { transform: scale(1.12); }
.avatar-swatch-active { border-color: var(--color-text-primary); }
```

---

## 4. Files to Modify

| File | Change |
|---|---|
| `crates/vexboard-frontend/src/pages/settings.rs` | Responsive grid, section icons, restyled nav option buttons |
| `crates/vexboard-frontend/src/components/user_menu.rs` | Avatar colour signal + swatch picker in modal |
| `crates/vexboard-frontend/style/main.css` | New CSS classes listed above |

---

## 5. Dependencies

No new Cargo dependencies. No Context7 lookup required. All changes are pure Leptos view markup and CSS. The `web_sys::window()...local_storage()` API is already used in `settings.rs` for theme persistence.

---

## 6. Build / Test Commands (Phase 3)

Per CLAUDE.md safe commands only:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`

WASM frontend build (`trunk build`) is NOT run unless Trunk + `wasm32-unknown-unknown` are confirmed installed; correctness is verified via `cargo clippy`.

---

## 7. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `grid-column: 1 / -1` on user management breaks on very narrow screens | `auto-fit` + `minmax(340px, 1fr)` naturally collapses to 1 column, so the span is harmless |
| Avatar colour from localStorage not available on first render (before WASM init) | Default `#3b82f6` used as fallback; the signal is initialised in an `Effect::new` so it updates within the same frame |
| Hardcoded `.acct-modal` background `#1e2a38` doesn't follow the CSS variable theme | Not in scope for this feature; noted for a future theme-consistency pass |
