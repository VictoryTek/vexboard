# Review: Fix stale "Add Quick Link" form state

## Diff Reviewed

`crates/vexboard-frontend/src/pages/dashboard/modals.rs` (lines 75-83):

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

Previously the closure body did not read `show_add_link_modal` at all — only `resolve_groups(&groups)` —
so the closure never re-ran on modal open/close, and a single long-lived `QuickLinkModal` instance (with
signals created once at `crates/vexboard-frontend/src/components/quick_link_modal.rs:49-54`) persisted
across every open, retaining stale field values. Confirmed via `git diff` against the committed baseline;
this is the only file changed (plus the untracked spec doc).

## Specification Compliance

The change matches `.github/docs/subagent_docs/add_quick_link_form_reset_spec.md` exactly — same file, same
lines, same wrapping construct (`show_add_link_modal.get().then(|| view! {...})`), no other files touched, no
changes to `quick_link_modal.rs`. **100% compliant.**

## Reactivity / Correctness Reasoning

- Leptos closures inside `view!` interpolation (`{move || ...}`) are tracked reactive scopes: they re-run
  whenever any signal read inside the closure body changes. The old closure read only `groups` (via
  `resolve_groups(&groups)`), so toggling `show_add_link_modal` (read only inside the child component's own
  `<Show when=move || visible.get()>`) never invalidated this outer closure — the child was constructed once
  and reused forever.
- The fix reads `show_add_link_modal.get()` directly in the outer closure, so the closure is now correctly
  reactive to modal open/close. `Option::then` returns `Some(view)` only when `true`, `None` when `false`.
  Leptos drops the previous view (and disposes its owned signals) when the interpolated value changes from
  `Some` to `None`, and constructs a brand-new `QuickLinkModal` (with fresh `signal(initial.title)` etc.,
  freshly seeded from empty defaults since `initial` is never passed for Add) each time it changes from `None`
  to `Some`. This directly fixes the reported bug: every reopen gets fresh, empty signals.
- This is functionally identical in shape to the pre-existing "Edit Quick Link" pattern (`modals.rs:120-149`,
  `edit_link_target.get().map(...)`), which already demonstrated the create/destroy-per-open lifecycle works
  correctly in this codebase — good precedent-following rather than inventing a new reset mechanism (e.g. an
  effect that manually clears signals, which would have been more code and more error-prone).
- Using the same `show_add_link_modal` signal for both the mount-gate (outer `.then()`) and the `visible` prop
  (passed straight through to the child's own internal `<Show>`) is slightly redundant — the inner `<Show>`'s
  `when` condition is always `true` at the moment the child exists — but this is harmless: no observable
  behavior difference, and it keeps the child component's public API (`visible: Signal<bool>` is required,
  not optional) unchanged, avoiding a wider change footprint. This mirrors how Edit passes a same-purpose
  `show_edit` signal that is likewise always `true` while mounted.
- Verified `show_add_link_modal` has exactly one writer setting it `true` (the "Add" button in
  `pages/dashboard/mod.rs:324`) and two writers setting it `false` (`on_close` in `modals.rs:79`,
  `on_save_link`'s save-success handler in `modals.rs:52`) — no other code path reads or depends on the modal
  remaining mounted while hidden, so gating construction on this signal introduces no regression elsewhere.

## Best Practices

Idiomatic Leptos: conditional mounting via a boolean-gated `Option`-returning closure is a standard, documented
pattern for "construct fresh on show, dispose on hide" component lifecycles, and is consistent with `Option::then`
usage already present in the file for the sibling Edit modals (which use `.map`, an equivalent idiom for
`Option`-typed signals). No `unsafe`, no unnecessary clones, no new allocations beyond what already existed.
**Grade: A.**

## Consistency

Directly mirrors the Edit-modal construction pattern already in the same file, using the more appropriate
idiom for a `bool` signal (`.then()`) rather than forcing an `Option` wrapper. Comment style, formatting, and
closure shape match the rest of `modals.rs`. **Grade: A.**

## Maintainability

Single-line semantic change plus an updated, accurate comment explaining *why* (fresh instance per open — this
directly documents the root-cause fix for future readers, preventing regression). No added complexity,
no new abstractions, no speculative flexibility. **Grade: A.**

## Completeness

The change is a complete, self-contained fix for the reported bug — reasoned through above via Leptos's
reactive-tracking and conditional-rendering semantics. No follow-up changes needed in `quick_link_modal.rs`
per the spec's analysis, which this review confirms by inspection (signals are correctly seeded from
`initial.unwrap_or(default)` at construction time — the previous bug was purely a parent mount-lifecycle
issue, not a component-internal one). **Grade: A.**

## Security

No security-relevant surface touched — purely client-side form-lifecycle change, no new external input
handling, no changes to API calls, auth, or serialization. **Grade: A (N/A, no concerns).**

## Performance

Negligible: constructing/disposing one small modal component's signals on open/close (a low-frequency,
user-driven event) instead of keeping it permanently mounted-but-hidden. This is the same cost profile as the
already-shipped Edit modal pattern, which has not been a performance concern. No regression. **Grade: A.**

## Build Validation (safe commands only, per spec/CLAUDE.md)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Exit 0 — no formatting diffs |
| `cargo clippy --workspace -- -D warnings` | Exit 0 — `Finished` cleanly, no warnings |
| `cargo test -p vexboard-server` | Exit 0 — 36 passed; 0 failed; 0 ignored |
| `cargo build --release --bin vexboard-server` | Exit 0 — `Finished` release profile |

No forbidden commands were run. `trunk build`/`trunk serve` were not attempted (frontend WASM compilation
cannot be verified natively per project constraints); correctness was instead verified by careful manual
reasoning through Leptos's reactive-closure and conditional-rendering semantics, cross-referenced against the
already-proven Edit-modal pattern in the same file.

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (100%)**

## Result: PASS
