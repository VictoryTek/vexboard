# Phase 1 Spec: Dark/Light Mode — localStorage Persistence

**Feature:** theme_persist  
**Date:** 2026-06-06  
**Audit Entry:** 2.2.10

---

## Current State

- `crates/vexboard-frontend/index.html`: `<html lang="en" class="dark">` — hardcoded dark, never updated at runtime.
- `crates/vexboard-frontend/src/pages/settings.rs:75–93`: A "Toggle Theme" button toggles `dark`/`light` CSS classes on `document.documentElement` in-place but never persists to `localStorage`. The chosen theme resets to dark on every page reload.
- No code reads `localStorage` at startup to restore the saved theme.

## Problem

The theme toggle is a session-only DOM mutation. Users who prefer light mode must re-toggle on every visit.

## Proposed Solution

Two targeted changes, no new dependencies:

### 1. `index.html` — inline theme-restore script

Add a `<script>` tag in `<head>` **before the Trunk link** so the correct class is applied synchronously before WASM loads, preventing a flash-of-wrong-theme (FOWT):

```html
<script>
  (function() {
    var t = localStorage.getItem('vexboard-theme');
    if (t === 'light') {
      document.documentElement.classList.remove('dark');
      document.documentElement.classList.add('light');
    }
    // default is dark (already set as class="dark")
  })();
</script>
```

### 2. `settings.rs` — persist on toggle

After updating the class list, call `localStorage.setItem`:

```rust
let win = web_sys::window().unwrap();
let store = win.local_storage().ok().flatten();
let theme_key = "vexboard-theme";
if is_dark {
    html.class_list().remove_1("dark").ok();
    html.class_list().add_1("light").ok();
    if let Some(s) = &store { let _ = s.set_item(theme_key, "light"); }
} else {
    html.class_list().remove_1("light").ok();
    html.class_list().add_1("dark").ok();
    if let Some(s) = &store { let _ = s.set_item(theme_key, "dark"); }
}
```

## Implementation Steps

1. Edit `crates/vexboard-frontend/index.html` — add inline script in `<head>`
2. Edit `crates/vexboard-frontend/src/pages/settings.rs` — add `localStorage.setItem` call in toggle handler

## Dependencies

None new. `web_sys::Window::local_storage()` is already available via the `web-sys` dependency (requires `"Storage"` feature — verify in `Cargo.toml`).

## Build / Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `scripts/preflight.sh`

## Risks

- If `web-sys` `"Storage"` feature is not listed in `vexboard-frontend/Cargo.toml`, add it.
- Trunk treats `<script>` in `<head>` as pass-through — no special handling needed.
