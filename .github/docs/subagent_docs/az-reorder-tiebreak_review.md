# BUG-9 — Default-View Drag Reorder Manipulates Wrong Item on `sort_order` Ties — Review

## Summary

Implementation matches spec exactly: the default (unsectioned) view's `on:drop` handler in
`crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` now applies the identical
`sort_by(sort_order, then display_name.to_lowercase())` tiebreak to the freshly-fetched `current`
list before computing `remove`/`insert` indices — the exact same expression already used
correctly by the grouped (`EitherOf4::B`) and by-source (`EitherOf4::C`) views' drop handlers,
and by this same view's own render-time sort. Confirmed via `Edit`'s uniqueness requirement that
only the one handler missing the tiebreak was matched and changed; the two already-correct
sectioned handlers were untouched.

## Build & Test Results (verbatim)

Since this is a WASM-only frontend change, the Approved backend commands
(`fmt`/`clippy --workspace`/`test -p vexboard-server`/`release build`) don't exercise this file
at the WASM target. As supplementary due diligence (`trunk` is not installed in this
environment, so `trunk build`/`trunk serve` were correctly not run per FORBIDDEN COMMANDS; the
`wasm32-unknown-unknown` target *is* installed, so a scoped, non-forbidden `cargo
check`/`clippy --target wasm32-unknown-unknown -p vexboard-frontend` was run instead):

`cargo check --target wasm32-unknown-unknown -p vexboard-frontend`:
```
    Checking vexboard-frontend v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.39s
```
Exit 0, clean compile for the actual WASM target.

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --target wasm32-unknown-unknown -p vexboard-frontend -- -D warnings`:
```
    Checking vexboard-frontend v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.59s
```
Exit 0, no warnings.

`cargo clippy --workspace -- -D warnings` (Approved command, confirms nothing else broke):
```
    Checking vexboard-frontend v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.30s
```
Exit 0.

`cargo test -p vexboard-server` — 34/34 passed, exit 0 (unaffected, no backend files changed).

`cargo build --release --bin vexboard-server` — exit 0 (unaffected, no backend files changed).

## Review Against Criteria

1. **Specification Compliance** — exact match to spec.
2. **Best Practices** — reuses an already-proven, in-file expression verbatim rather than
   inventing a new sort approach, minimizing risk of introducing a subtly different tiebreak.
3. **Consistency** — all three drag-and-drop views (`EitherOf4::B`, `C`, `D`) now apply the
   identical sort before index-based mutation, closing the one inconsistency that caused this
   bug.
4. **Maintainability** — no new abstraction introduced; the fix is a single added line matching
   the existing style precisely.
5. **Completeness** — fully resolves BUG-9; verified (via grep during Phase 1) that the two
   sectioned views already had this tiebreak, so no other call site needed the same fix.
6. **Performance** — negligible; one additional in-memory sort of an already-fetched small list
   (self-hosted dashboard scale), matching the cost already paid by the two sectioned views for
   the same operation.
7. **Security** — none; purely a UI correctness fix.
8. **API Currency** — n/a, no external API involved.
9. **Build Validation** — WASM-target compile and clippy both clean; backend Approved commands
   confirm no unrelated regression, consistent with FORBIDDEN COMMANDS constraints (no
   `trunk build`/`trunk serve` run, since `trunk` is not installed here).

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

## Returns

- Build result: PASS (WASM-target check/clippy clean; backend fmt/clippy/tests/release build
  clean and unaffected)
- **PASS**
