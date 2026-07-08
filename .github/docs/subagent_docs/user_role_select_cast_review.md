# Fix Role `<select>` DOM Cast — Review (BUG-2)

Spec: `user_role_select_cast_spec.md`

## Modified Files

- `crates/vexboard-frontend/Cargo.toml` — added `"HtmlSelectElement"` to the
  `web-sys` feature list
- `crates/vexboard-frontend/src/pages/settings.rs` — role `<select>`'s `on:change`
  handler now casts to `web_sys::HtmlSelectElement` instead of `HtmlInputElement`

## Review Against Spec

1. **Specification compliance** — exact one-purpose fix: feature flag added, cast
   type corrected. Nothing else touched.
2. **Best practices** — matches the existing per-element-type casting pattern used
   by the adjacent username/password `<input>` handlers in the same file.
3. **Consistency** — feature list ordering/style in `Cargo.toml` unchanged aside from
   the one insertion.
4. **Completeness** — closes the exact defect: `new_role` now updates on selection
   change, so the "Admin" option is no longer inert and new users are created with
   the role actually selected in the dropdown.
5. **Performance/Security** — not applicable; no behavior change beyond the fix.
6. **API currency** — `web_sys::HtmlSelectElement::value()` is the standard,
   long-stable web-sys API mirroring `HtmlInputElement::value()`; no deprecated
   pattern involved.

## Build Validation (verbatim)

**`cargo fmt --all -- --check`** — clean, no diff.

**`cargo clippy --workspace -- -D warnings`**
```
    Checking vexboard-frontend v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.18s
```
No warnings — confirms `web_sys::HtmlSelectElement` resolves correctly with the new
feature flag and the cast type-checks against `EventTarget`.

**`cargo test -p vexboard-server`** — 34/34 pass (backend-only; unaffected by this
frontend-only change).

**`cargo build --release --bin vexboard-server`** — succeeds (unaffected).

**Not run:** `trunk build` (FORBIDDEN COMMANDS) and no live browser check possible in
this environment; clippy's native type-check is the strongest available signal for
this WASM-only crate per project constraints.

## Score Table

| Category                  | Score | Grade |
|----------------------------|-------|-------|
| Specification Compliance   | 100%  | A     |
| Best Practices              | 100%  | A     |
| Functionality                | 100%  | A     |
| Code Quality                 | 100%  | A     |
| Security                     | N/A   | —     |
| Performance                  | 100%  | A     |
| Consistency                   | 100%  | A     |
| Build Success                 | 100%  | A     |

**Overall Grade: A (100%)**

## Result

**PASS** — proceeding to Phase 6 (Preflight; already run above, exit code 0).
