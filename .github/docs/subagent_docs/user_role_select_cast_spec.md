# Fix Role `<select>` DOM Cast — Spec (BUG-2)

Source: MASTER_PLAN.md HIGH PRIORITY / Data Loss / Functional Breakage / BUG-2 (B-H6)

## Current State Analysis

`crates/vexboard-frontend/src/pages/settings.rs`:

- `new_role` signal defaults to `"viewer"` (line 38).
- The Add User form's role `<select>` (lines 330-342) has an `on:change` handler that
  casts the event target to `web_sys::HtmlInputElement` (line 335) and calls
  `set_new_role.set(el.value())` only if the cast succeeds.
- The actual DOM element behind a `<select>` is `HTMLSelectElement`, not
  `HTMLInputElement`. `dyn_into::<HtmlInputElement>()` on an `HtmlSelectElement`
  always returns `Err`, so the `if let Some(el) = ...` branch never executes.
  `new_role` therefore never changes from its `"viewer"` default regardless of what
  the admin picks in the dropdown — the "Admin" `<option>` is completely inert.
- `web-sys` (`crates/vexboard-frontend/Cargo.toml:15`) does not currently enable the
  `HtmlSelectElement` feature (only `HtmlInputElement` and others are listed), so
  `web_sys::HtmlSelectElement` is not even available to reference yet.
- This is the exact same silent-failure shape already correctly avoided elsewhere in
  this file: the two adjacent `<input>` handlers (username/password, lines 310-316,
  323-329) correctly cast to `HtmlInputElement` because those *are* `<input>`
  elements.

## Problem Definition

New users are always created with `role: "viewer"` no matter what an admin selects,
because the DOM cast used to read the `<select>`'s value is for the wrong element
type and silently fails every time.

## Proposed Solution

1. Enable the `HtmlSelectElement` feature on the `web-sys` dependency
   (`crates/vexboard-frontend/Cargo.toml:15`).
2. Change the `on:change` handler's cast target from `HtmlInputElement` to
   `HtmlSelectElement`:
   ```rust
   on:change=move |ev| {
       use wasm_bindgen::JsCast;
       if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
           set_new_role.set(el.value());
       }
   }
   ```

No other lines in this handler change — `.value()` exists on both element types with
the same signature, so only the cast target type changes.

## Implementation Steps

1. `crates/vexboard-frontend/Cargo.toml` — add `"HtmlSelectElement"` to the `web-sys`
   `features` list.
2. `crates/vexboard-frontend/src/pages/settings.rs:335` — change
   `dyn_into::<web_sys::HtmlInputElement>()` to
   `dyn_into::<web_sys::HtmlSelectElement>()`.

## Dependencies

`web-sys` is already a workspace dependency; this only adds a feature flag to an
existing dependency (no version change, no new crate) — Context7 not required per
CLAUDE.md's exemption for changes with no new dependency addition.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** None identified — strict bugfix; the `<option value="viewer" selected=true>`
  default and the `<option value="admin">` are unchanged, only the mechanism that
  reads the selected value now actually works.
- **Note:** Per CLAUDE.md constraints, `vexboard-frontend` is WASM-only; validation
  here is `cargo fmt --all -- --check` and `cargo clippy --workspace -- -D warnings`
  (both natively type-check this crate), not a `trunk build` (FORBIDDEN COMMANDS).

## Files

- `crates/vexboard-frontend/src/pages/settings.rs:330-342`
- `crates/vexboard-frontend/Cargo.toml`
