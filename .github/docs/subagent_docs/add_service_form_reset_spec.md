# Spec: Fix stale "Add Service" form state

## Current State Analysis

Same root cause class as the already-fixed quick-link bug (see `add_quick_link_form_reset_spec.md` /
`add_quick_link_form_reset_review.md`). `crates/vexboard-frontend/src/pages/dashboard/modals.rs:66-74`
renders the "Add Service" modal (`EditModal`) via a closure that only tracks `resolve_groups(&groups)`:

```rust
// Add service modal — reactive wrapper so groups prop updates when resource loads
{move || view! {
    <EditModal
        visible=show_modal
        on_close=Callback::new(move |_| show_modal.set(false))
        on_save=on_save
        groups=resolve_groups(&groups)
    />
}}
```

`show_modal` is only read inside the child via the `visible` prop, so toggling it never invalidates this
closure. `EditModal`'s internal form signals (declared once at component-creation time, analogous to
`QuickLinkModal`) therefore persist across every open/close cycle of "Add Service" — reproducing the same
stale-form bug now reported for services. The already-applied fix to the sibling "Add Quick Link" modal
(`modals.rs:76-84`, gating construction on `show_add_link_modal.get()`) resolved that case; "Add Service"
was never given the equivalent fix.

The "Edit Service" modal (`modals.rs:87-119`) is unaffected — it's built via `edit_target.get().map(...)`,
which already creates a fresh `EditModal` instance per edit.

## Problem Definition

The "Add Service" modal reuses a single `EditModal` instance/signal set across every add, so field values
from a previous add attempt persist into the next one until a page refresh.

## Proposed Solution

Apply the identical fix pattern used for "Add Quick Link": gate the "Add Service" modal's construction on
`show_modal.get()` so a fresh `EditModal` instance (fresh signals) is created each time it opens and dropped
when closed.

```rust
// Add service modal — fresh instance per open so form state doesn't leak between adds
{move || show_modal.get().then(|| view! {
    <EditModal
        visible=show_modal
        on_close=Callback::new(move |_| show_modal.set(false))
        on_save=on_save
        groups=resolve_groups(&groups)
    />
})}
```

## Implementation Steps

1. Edit `crates/vexboard-frontend/src/pages/dashboard/modals.rs:66-74`: wrap the `EditModal` view in
   `show_modal.get().then(|| ...)`.
2. No other files require changes.

## Dependencies

None — internal Leptos component-lifecycle fix, no new dependency, no Context7 lookup required.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Same class of change as the already-reviewed quick-link fix (PASS, 100%), so risk profile is
  identical and already validated: no CSS transition tied to the modal's mount/unmount, and `on_close`
  already sets `show_modal` false today so unmounting on close is not a new behavior change, only fixes
  stale state — a positive side effect.
- **Risk:** Confirm `show_modal` isn't read/depended on elsewhere in a way that assumes `EditModal` stays
  mounted while hidden — to be verified in Phase 3 review via grep, same as was done for `show_add_link_modal`.

## Approved Validation Commands (Phase 3/6)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`
