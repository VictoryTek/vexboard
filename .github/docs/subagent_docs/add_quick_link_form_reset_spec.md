# Spec: Fix stale "Add Quick Link" form state

## Current State Analysis

`QuickLinkModal` (`crates/vexboard-frontend/src/components/quick_link_modal.rs:32-199`) declares its form
signals (`name`, `url`, `icon`, `icon_auto`, `desc`, `selected_group_id`, lines 49-54) once, at component
creation time, seeded from the `initial` prop (or empty defaults, lines 41-47). Visibility is controlled
purely by a `<Show when=move || visible.get()>` (line 57) — hiding the modal does not destroy it or its
signals, only its DOM output.

In `crates/vexboard-frontend/src/pages/dashboard/modals.rs:76-83`, the "Add Quick Link" modal is instantiated
inside a reactive closure that only re-runs when `resolve_groups(&groups)` changes:

```rust
{move || view! {
    <QuickLinkModal
        visible=show_add_link_modal
        on_close=Callback::new(move |_| show_add_link_modal.set(false))
        on_save=on_save_link
        groups=resolve_groups(&groups)
    />
}}
```

Toggling `show_add_link_modal` is read inside the child (`visible` prop), not in this closure body, so it
never invalidates the closure. The result: one long-lived `QuickLinkModal` instance for "Add", whose signals
persist across every open/close cycle — reproducing the reported bug (previous entry's title/url/icon/description
still populated on next open).

By contrast, the "Edit Quick Link" modal (`modals.rs:120-149`) is built via
`edit_link_target.get().map(|(id, initial)| { ... view! { <QuickLinkModal .../> } })`. Because this closure
reads `edit_link_target` (an `Option`), a fresh `QuickLinkModal` instance (fresh signals) is created every time
an edit target is set, and the instance is dropped when `edit_link_target` becomes `None`. This is why Edit
does not exhibit the bug.

## Problem Definition

The "Add Quick Link" modal reuses a single component instance/signal set across every add, instead of getting
a fresh instance per open, so field values from a previous add leak into the next add attempt.

## Proposed Solution

Make the "Add Quick Link" modal follow the same create/destroy-per-open pattern already used by "Edit Quick
Link", instead of introducing a new signal-reset mechanism. This is the most surgical, most consistent-with-
existing-code fix (Edit already proves the pattern works and is idiomatic in this codebase).

Change `modals.rs:76-83` from a closure that only reacts to `groups`, to one gated on
`show_add_link_modal.get()`, so a brand new `QuickLinkModal` (and therefore brand new, empty signals) is
constructed each time the modal is opened, and dropped when closed.

```rust
// Add quick link modal — fresh instance per open so form state doesn't leak between adds
{move || show_add_link_modal.get().then(|| view! {
    <QuickLinkModal
        visible=show_add_link_modal
        on_close=Callback::new(move |_| show_add_link_modal.set(false))
        on_save=on_save_link
        groups=resolve_groups(&groups)
    />
})}
```

No changes are required in `quick_link_modal.rs` — its signal initialization from `initial`/defaults already
works correctly; the bug is purely in the parent's mount lifecycle for the Add case.

## Implementation Steps

1. Edit `crates/vexboard-frontend/src/pages/dashboard/modals.rs:76-83`: wrap the `QuickLinkModal` view in
   `show_add_link_modal.get().then(|| ...)` so it's only constructed while the modal is meant to be open.
2. No other files require changes.

## Dependencies

None — no new crates, no external library changes. This is an internal Leptos component-lifecycle fix; no
Context7 lookup required per policy (internal code change, no new dependency).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Removing the inner `<Show>` gate reliance could change modal open/close animation/transition
  behavior. **Mitigation:** `QuickLinkModal` has no CSS transition tied to the `<Show>` toggle (it's a plain
  conditional render, `style="display:flex"` block appears/disappears instantly) — confirmed by reading
  `quick_link_modal.rs:56-197`, so behavior is visually identical, just with fresh state.
- **Risk:** `on_close` (Cancel button / backdrop click) also now unmounts the modal instead of just hiding it.
  Functionally equivalent since `show_add_link_modal.set(false)` already gates the `<Show>` today, and it
  additionally clears any partially-entered data on cancel — this is a positive side effect matching the bug
  report's intent (no stale state), not a regression.

## Approved Validation Commands (Phase 3/6)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server` (frontend crate is WASM-only; no native test coverage for this component)
- `cargo build --release --bin vexboard-server`

Note: this change touches only `vexboard-frontend`, which cannot be compiled/tested natively. Verification
will rely on `cargo fmt`/`clippy` where applicable to the workspace check level, and manual code reading, since
`trunk build`/`trunk serve` are forbidden without confirming Trunk + `wasm32-unknown-unknown` are installed.
