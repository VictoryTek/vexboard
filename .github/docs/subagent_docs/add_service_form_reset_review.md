# Review: Fix stale "Add Service" form state

## Diff Reviewed

`crates/vexboard-frontend/src/pages/dashboard/modals.rs` (lines 66-74), confirmed via `git diff`:

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

replacing the previous unconditional closure that only read `resolve_groups(&groups)`. This is the only
functional change in the working tree; the untracked `add_service_form_reset_spec.md` doc is the Phase 1
spec. An unrelated, pre-existing `skip_tls_verify` field is present in `EditFormData`/`on_save`'s JSON body
in both old and new code — not part of this diff, confirmed not touched by the change.

## Specification Compliance

Matches `.github/docs/subagent_docs/add_service_form_reset_spec.md` exactly — same file, same lines, same
`show_modal.get().then(|| view! {...})` wrapping construct, no other files touched. **100% compliant.**

## Reactivity / Correctness Reasoning

- The old closure at `modals.rs:66-74` read only `resolve_groups(&groups)` inside its body. `show_modal` was
  read solely by the child `EditModal`'s own internal `<Show when=move || visible.get()>` (or equivalent),
  which is a *different* reactive scope than the outer interpolation closure. Because the outer closure never
  read `show_modal`, toggling it did not invalidate/re-run the outer closure, so Leptos never disposed and
  reconstructed the `EditModal` instance — the same long-lived instance (and its internally-created signals,
  seeded once from `initial.unwrap_or(default)` at `modal_edit.rs:47-56`) persisted across every open, retaining
  whatever the user typed on the previous add.
- The fix reads `show_modal.get()` directly in the outer closure via `.then()`. This makes the outer closure's
  reactive scope track `show_modal`, so it correctly re-runs on every open/close transition. `Option::then`
  yields `None` while closed (Leptos drops the previous view and disposes its owned signals) and `Some(view!)`
  on open (Leptos constructs a brand-new `EditModal`, running `let initial = initial.unwrap_or(EditFormData {
  ..default })` and `signal(initial.display_name)` etc. fresh each time). This directly resolves the reported
  bug: every reopen starts from empty defaults.
- Verified against `EditModal`'s definition (`modal_edit.rs:38-56`): `initial` is an `Option<EditFormData>`
  prop, and the "Add Service" call site never passes `initial`, so a fresh mount always defaults to empty
  fields — confirming the fix's correctness end-to-end, not just at the mount/unmount boundary.
- This mirrors the pre-existing "Edit Service" pattern in the same file (`modals.rs:87-119`,
  `edit_target.get().map(...)`), which already demonstrates correct create/destroy-per-open semantics in this
  codebase, and is identical in shape/idiom to the already-approved "Add Quick Link" fix
  (`modals.rs:76-84`, PASS/100% per `add_quick_link_form_reset_review.md`). Same root cause class, same fix
  pattern, same file — no new mechanism invented.
- Using `show_modal` for both the outer mount-gate and the `visible` prop is slightly redundant (the child's
  internal visibility check is always `true` once mounted), but this is harmless and matches the already-shipped
  quick-link fix's accepted tradeoff: it preserves the child's existing public API (`visible: Signal<bool>`)
  unchanged rather than widening the change's footprint.

## Grep Verification — `show_modal` Usage

```
crates/vexboard-frontend/src/pages/dashboard/modals.rs:15   RwSignal<bool> parameter declaration
crates/vexboard-frontend/src/pages/dashboard/modals.rs:36   on_save success handler: show_modal.set(false)
crates/vexboard-frontend/src/pages/dashboard/modals.rs:67   outer gate: show_modal.get().then(...)
crates/vexboard-frontend/src/pages/dashboard/modals.rs:69   visible=show_modal prop
crates/vexboard-frontend/src/pages/dashboard/modals.rs:70   on_close: show_modal.set(false)
crates/vexboard-frontend/src/pages/dashboard/mod.rs:135     let show_modal: RwSignal<bool> = RwSignal::new(false);
crates/vexboard-frontend/src/pages/dashboard/mod.rs:190     show_modal=show_modal (passed into DashboardModals)
crates/vexboard-frontend/src/pages/dashboard/mod.rs:304     "Add Service" button: show_modal.set(true)
```

Exactly one writer sets it `true` (the "Add Service" button), and two writers set it `false` (`on_close`,
and the save-success handler). No other code path reads or depends on `EditModal` remaining mounted while
hidden — gating construction on this signal introduces no regression elsewhere. Same verification pattern
used and passed for the sibling quick-link fix.

## Best Practices

Idiomatic Leptos: boolean-gated `Option`-returning closure (`.then()`) for "construct fresh on show, dispose
on hide" component lifecycle is a standard pattern, consistent with `Option::then`/`.map()` usage already
present elsewhere in this same file for the Edit modals. No `unsafe`, no unnecessary clones, no new
allocations beyond what already existed. **Grade: A.**

## Consistency

Directly mirrors both the pre-existing "Edit Service" construction pattern and the already-approved "Add
Quick Link" fix in the same file. Comment style ("fresh instance per open so form state doesn't leak between
adds") is copied verbatim from the quick-link fix's comment, keeping documentation consistent across sibling
modals. Formatting and closure shape match the rest of `modals.rs`. **Grade: A.**

## Maintainability

Single-line semantic change (unconditional closure → `.then()`-gated closure) plus an accurate, updated
comment explaining the why. No added complexity, no new abstractions, no speculative flexibility. Directly
traceable to the user's request. **Grade: A.**

## Completeness

Self-contained fix, fully addressing the reported symptom (reasoned through above via Leptos's
reactive-closure and conditional-mounting semantics). No follow-up changes needed in `modal_edit.rs` — its
signals are already correctly seeded from `initial.unwrap_or(default)` at construction time, so the previous
bug was purely a parent mount-lifecycle issue, not a component-internal one, matching the spec's analysis.
**Grade: A.**

## Security

No security-relevant surface touched — purely a client-side form-lifecycle change; no new external input
handling, no changes to API calls, auth, or serialization. The unrelated pre-existing `skip_tls_verify` field
visible in the diff context is untouched by this change. **Grade: A (N/A, no concerns).**

## Performance

Negligible: constructing/disposing one small modal component's signals on a low-frequency, user-driven
open/close event, instead of keeping it permanently mounted-but-hidden. Same cost profile as the
already-shipped Edit-modal and Add-Quick-Link patterns, which have not been a performance concern. No
regression. **Grade: A.**

## Build Validation (safe commands only, per spec/CLAUDE.md)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Exit 0 — no formatting diffs |
| `cargo clippy --workspace -- -D warnings` | Exit 0 — `Finished` cleanly, no warnings |
| `cargo test -p vexboard-server` | Exit 0 — 36 passed; 0 failed; 0 ignored |
| `cargo build --release --bin vexboard-server` | Exit 0 — `Finished` release profile |

No forbidden commands were run (no bare `cargo build`, no `--workspace` native build, no `trunk build`/`trunk
serve`). Frontend WASM compilation was not attempted natively per project constraints; correctness was
verified by careful manual reasoning through Leptos's reactive-closure and conditional-rendering semantics,
cross-referenced against the already-proven Edit-modal pattern and the already-approved, identically-shaped
quick-link fix in the same file.

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
