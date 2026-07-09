# User Menu Click-Outside Spec

## Current State Analysis

`crates/vexboard-frontend/src/components/user_menu.rs` implements the `UserMenu`
component. The dropdown open/closed state is a single boolean signal
(`dropdown_open`) toggled only by the trigger button's `on:click` handler
(`user_menu.rs:157-158`). There is no listener for clicks elsewhere in the
document, so once open, the dropdown only closes via:

- clicking the trigger button again (toggle)
- clicking "Account Settings" (explicitly calls `set_dropdown_open.set(false)`)
- clicking "Logout"

Clicking anywhere else on the page leaves `dropdown_open` at `true` and the
`.user-menu-dropdown.open` CSS class stays applied, so the menu visually stays
open. This matches the reported bug.

No existing "click outside" pattern exists elsewhere in the frontend codebase
(confirmed via grep for `click_outside`, `mousedown`, `window_event_listener`,
`closest`).

## Problem Definition

The user account dropdown does not close when the user clicks outside of it.
Expected behavior: clicking anywhere outside the open dropdown (and outside
the trigger button, to avoid double-toggling) should close it.

## Proposed Solution

Use Leptos's `window_event_listener` helper (`leptos::prelude::window_event_listener`,
available in leptos 0.8, re-exported from `leptos_dom`) to attach a `click`
listener on the `window` for the lifetime of the component. It is automatically
cleaned up when the component's reactive scope is disposed, matching Leptos's
idiomatic pattern for global listeners (no manual `add_event_listener`/cleanup
needed).

On each window click:
1. If the dropdown isn't open, do nothing.
2. Otherwise, check whether the click's target is contained within the
   `user-menu` container (trigger button + dropdown) using a `NodeRef` on the
   outer `<div class="user-menu">` wrapper and `Node::contains`.
3. If the click target is outside that container, close the dropdown
   (`set_dropdown_open.set(false)`).

This avoids a separate mousedown/click race with the trigger's own `on:click`
toggle handler, since Leptos dispatches DOM events synchronously in the order
they fire, and the trigger button click is itself inside the container so the
window listener will treat it as "inside" and leave the toggle handler as the
sole source of truth for that case.

## Implementation Steps

1. Add a `NodeRef<leptos::html::Div>` for the `user-menu` wrapper div and
   attach it via `node_ref=menu_ref`.
2. Add `window_event_listener(ev::click, move |ev| { ... })` inside the
   component body (added once, during component setup, so it's registered
   exactly once per mount and cleaned up on unmount).
3. In the handler: if `dropdown_open.get_untracked()` is `false`, return.
   Otherwise get the event target as `web_sys::Node` (`ev.target()` ->
   `dyn_into::<web_sys::Node>()`), and check
   `menu_ref.get_untracked().map(|el| !el.contains(target.as_ref()))`.
   If the click is outside (or the ref/target is missing), call
   `set_dropdown_open.set(false)`.
4. No new web-sys features are required beyond what's needed for `Node`;
   `HtmlElement`/`Node` casting needs the `web-sys` `"Node"` feature enabled
   (currently not listed in `Cargo.toml`) — add it.

## Dependencies

No new crates. Requires enabling the existing `web-sys` optional feature
`"Node"` in `crates/vexboard-frontend/Cargo.toml` (`web-sys` is already a
direct dependency; `Node` is a standard web-sys feature flag, no external
research needed via Context7 since this is a first-party `web-sys` feature,
not a new library).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Listener firing on every click app-wide could be a minor perf
  concern. Mitigation: handler exits immediately when dropdown is closed
  (`get_untracked` check first), so cost when idle is negligible.
- **Risk:** Interfering with the trigger button's own toggle. Mitigation:
  trigger button is inside the `user-menu` container, so the window listener
  treats those clicks as "inside" and does not double-close; the existing
  `on:click` toggle on the button remains the only handler that reopens/
  recloses via the button.
- **Risk:** `ev.target()` may be `None` or not castable in edge cases (e.g.
  clicking on a text node). Mitigation: treat missing/uncastable target as
  "outside" and close, which is the safe default for a dropdown.

## Approved validation commands (per CLAUDE.md, this project cannot build the
WASM frontend without confirming Trunk + wasm32 target are installed)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings` (note: this is listed as a
  "safe" command in CLAUDE.md's Phase 3 approved list, but the project's own
  Resource Constraints section says the frontend crate is WASM-only and
  cannot compile for native targets — `cargo clippy --workspace` would
  attempt to compile it. This will be flagged in Phase 3; the safer
  equivalent used instead is `cargo clippy -p vexboard-frontend --target
  wasm32-unknown-unknown -- -D warnings` if the wasm32 target is confirmed
  installed, otherwise skip and rely on `cargo fmt` plus manual code
  inspection for this frontend-only change).
