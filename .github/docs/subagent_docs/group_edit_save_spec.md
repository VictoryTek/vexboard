# Group Edit Save — Spec

## Current State Analysis

`crates/vexboard-frontend/src/components/modal_groups.rs` implements the "Manage
Groups" modal (`GroupsModal`). Each group row has a pencil ("Rename") button that
sets `editing_id` to the group's id, switching the row into inline-edit mode:

- The name label becomes a text `<input>`.
- The color dot becomes a `ColorSwatches` picker.

The only code paths that call `do_rename(id)` (the function that PUTs the new
`name`/`color` to `/api/v1/groups/{id}`) are:

- `on:blur` on the name `<input>` (`modal_groups.rs:300`)
- `Enter` keydown on the name `<input>` (`modal_groups.rs:304`)

There is no button in the edit-mode UI that explicitly saves, and clicking a
color swatch (`ColorSwatches::on_select`, `modal_groups.rs:277`) only updates
the `edit_color` signal — it never touches the input or calls `do_rename`.

## Problem Definition

A user who opens edit mode and only changes the color (never focuses/blurs the
name input, never presses Enter) has no way to persist the change — there is
no visible affordance to save, and the implicit blur/Enter save is not
discoverable. This matches the reported bug: "I can change things like name
and color but there is no way to save those edited changes."

## Proposed Solution

Add explicit Save (checkmark) and Cancel (x) buttons to the row when
`is_editing()` is true, replacing the Rename/Delete button pair for that row
in edit mode. This makes committing changes an explicit, discoverable action
regardless of which field (name or color) was touched, while keeping the
existing Enter-to-save and Escape-to-cancel keyboard behavior as-is (no need
to remove them — they're a nice-to-have shortcut, not the bug).

`on:blur` on the input can stay — it doesn't hurt — but the fix must not rely
on it exclusively.

### Implementation Steps

1. In `modal_groups.rs`, in the per-row `view!`, wrap the existing
   Rename + Delete buttons in a conditional: when `is_editing()` is true,
   render Save + Cancel buttons instead; when false, render the existing
   Rename + Delete buttons unchanged.
2. Save button `on:click` calls `do_rename(id)`.
3. Cancel button `on:click` sets `editing_id.set(None)` (same as existing
   Escape behavior), discarding `edit_name`/`edit_color` changes.
4. Reuse existing SVG icon style conventions (13x13, stroke-based) already
   used for Rename/Delete in this file for visual consistency.

## Dependencies

None — no new crates or external libraries. Pure Leptos view/logic change in
an existing component.

## Configuration Changes

None.

## Risks and Mitigations

- Risk: breaking existing keyboard-based save/cancel (Enter/Escape) for users
  relying on it. Mitigation: leave those handlers untouched; only add
  buttons.
- Risk: layout shift in the row when switching from 2 buttons to 2 different
  buttons. Mitigation: keep same button sizing/spacing conventions already
  used in the file.

## Files Affected

- `crates/vexboard-frontend/src/components/modal_groups.rs`
