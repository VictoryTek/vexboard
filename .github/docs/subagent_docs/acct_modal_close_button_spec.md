# Account Settings Modal — Close (X) Button — Spec

## Current State Analysis
`UserMenu` (crates/vexboard-frontend/src/components/user_menu.rs:215-319) renders the
Account Settings modal as an `.acct-modal-overlay` > `.acct-modal` pair. The modal has
an `<h3>` title and a `.modal-actions` footer with only "Cancel" and (conditionally)
"Save" buttons. There is no way to dismiss the modal except clicking "Cancel", and no
click-outside-to-close or Escape-key handling either.

Styling for the modal lives in crates/vexboard-frontend/style/main.css:453-462.

## Problem
Once a user changes the avatar color (or any field) inside the modal, closing it
requires clicking "Cancel" — there is no explicit close affordance in the header, which
is a common UX expectation (top-right X).

## Proposed Solution
Add a close button (×) to the top-right corner of `.acct-modal`, next to/above the
title, that closes the modal using the same logic as the existing "Cancel" button
(reset modal_open, save_error, save_success signals). No new state is needed — reuse
the existing `set_modal_open`, `set_save_error`, `set_save_success` signals.

### Markup change
Wrap the `<h3>` in a header row containing the title and a close button:
```html
<div class="acct-modal-header">
  <h3>Account Settings</h3>
  <button class="acct-modal-close" aria-label="Close" on:click=close_modal>"×"</button>
</div>
```
Extract the shared close logic into a local closure `close_modal` used by both the new
X button and the existing Cancel button to avoid duplicating the three signal resets.

### CSS change
Add to main.css near the other `.acct-modal` rules:
- `.acct-modal-header`: flex row, space-between, align-items center, margin-bottom matching current h3 margin.
- `.acct-modal-header h3`: margin 0.
- `.acct-modal-close`: no background/border, larger font-size for the ×, opacity ~0.7, hover opacity 1, cursor pointer, small padding, line-height 1.

## Implementation Steps
1. In `user_menu.rs`, define `close_modal` closure before the `view!` block that sets
   `modal_open`, `save_error`, `save_success` (same body currently inline in Cancel's
   `on:click`).
2. Replace `<h3>"Account Settings"</h3>` with the header div containing the h3 and the
   new close button, both wired to `close_modal`.
3. Update Cancel button's `on:click` to call `close_modal`.
4. Add CSS rules for `.acct-modal-header` / `.acct-modal-close` in main.css.

## Dependencies
None — no new crates, pure Leptos view + CSS change. Context7 not required per policy
(internal/styling-only change).

## Configuration Changes
None.

## Risks and Mitigations
- Risk: click on × inside overlay could bubble to overlay click-outside handling —
  there currently is no click-outside handler on the overlay, so no conflict.
- Risk: duplicate close logic drifting — mitigated by extracting `close_modal` closure
  used by both buttons.
