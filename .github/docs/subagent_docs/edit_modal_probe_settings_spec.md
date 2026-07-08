# Edit Service Modal — Preserve Probe Settings — Spec (BUG-1)

Source: MASTER_PLAN.md HIGH PRIORITY / Data Loss / Functional Breakage / BUG-1
(B-H3, A-H1)

## Current State Analysis

`crates/vexboard-frontend/src/components/modal_edit.rs`:

- `EditFormData` (line 28-37) already carries `probe_enabled: bool` and
  `probe_interval: i64`.
- The component receives the service's real values via the `initial: Option<EditFormData>`
  prop (populated by the caller when editing an existing service).
- Every other field (`display_name`, `description`, `url`, `icon`, `group_id`) is
  captured into its own reactive `signal()` from `initial` (lines 58-64) and read back
  out in the Save button's `on_save.run(EditFormData { ... })` call (lines 194-203).
- `probe_enabled`/`probe_interval` are **not** captured into signals and are **not**
  read back from `initial` at save time — the Save handler hardcodes
  `probe_enabled: true, probe_interval: 30` (lines 201-202) unconditionally.
- There is no form control for probing in the modal at all — the bug is entirely
  invisible to the user. Editing any other field (rename, icon, URL, group) silently
  re-enables probing on a service that was disabled, and resets any custom
  `probe_interval` back to the default 30s.

## Problem Definition

Saving an edit always overwrites `probe_enabled`/`probe_interval` with hardcoded
defaults instead of preserving whatever the service already had, causing silent data
loss on every edit.

## Proposed Solution

Capture `initial.probe_enabled` and `initial.probe_interval` as plain (non-reactive)
local bindings before `initial`'s other fields are moved into signals, then use those
bindings — not hardcoded literals — in the Save payload. No new signals are needed
since the modal has no UI to change these values (out of scope here; UI is tracked
separately as ARCH work / no master-plan bullet requests it for BUG-1 specifically —
the fix instruction is explicitly "pass initial.probe_enabled and initial.probe_interval
through to the save payload instead of hardcoding").

```rust
let initial = initial.unwrap_or(EditFormData {
    display_name: String::new(),
    description: String::new(),
    url: String::new(),
    icon: String::new(),
    group_id: None,
    probe_enabled: true,
    probe_interval: 30,
});
let initial_probe_enabled = initial.probe_enabled;
let initial_probe_interval = initial.probe_interval;

let (name, set_name) = signal(initial.display_name);
// ... unchanged ...
```

And at the Save handler:
```rust
on_save.run(EditFormData {
    display_name: name.get(),
    description: desc.get(),
    url: url.get(),
    icon: icon.get(),
    group_id: selected_group_id.get(),
    probe_enabled: initial_probe_enabled,
    probe_interval: initial_probe_interval,
});
```

Both fields are `Copy` (`bool`, `i64`), so binding them to plain `let`s before the
other (non-`Copy`) fields of `initial` are moved into signals is sufficient — no
`Cell`/`RwSignal` needed, and the `move` closure captures the two plain `i64`/`bool`
locals by copy like any other captured value.

## Implementation Steps

1. `crates/vexboard-frontend/src/components/modal_edit.rs` — add the two local
   bindings; replace the two hardcoded literals in the Save closure with them.

## Dependencies

None — no new crate, pure internal logic fix within an existing Leptos component.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** None identified — this is a strict bugfix restoring already-defined,
  already-plumbed-through data (`EditFormData` already has the fields; they're just
  not read from `initial` at save time).
- **Note:** Per CLAUDE.md constraints, `vexboard-frontend` is WASM-only and cannot be
  built/tested with `cargo test`/`cargo build` natively. Validation for this change is
  limited to `cargo fmt --all -- --check` and `cargo clippy --workspace -- -D warnings`
  (both of which do natively type-check the frontend crate, per the project's existing
  preflight script and this session's prior observation that `clippy --workspace`
  successfully checks `vexboard-frontend`). A full `trunk build` is not run per
  FORBIDDEN COMMANDS (Trunk/wasm32 target not confirmed installed).

## Files

- `crates/vexboard-frontend/src/components/modal_edit.rs:48-64,194-203`
