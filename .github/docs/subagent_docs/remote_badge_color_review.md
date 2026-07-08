# Remote Badge Color — Review

## Scope

Single change: replace `#5b8def` with `#ec4899` for the "Remote" source badge
in `crates/vexboard-frontend/src/components/service_card.rs` (two occurrences,
lines 58 and 68), per `remote_badge_color_spec.md`.

## Checks

1. **Specification Compliance** — Both occurrences updated exactly as specified. Match.
2. **Best Practices** — Matches existing inline-hex-string pattern used by the
   other three badges (Docker/Podman/Systemd); no new pattern introduced.
3. **Consistency** — Uses the same `Some((label, hex).to_string())` shape as
   sibling branches; no style deviation.
4. **Maintainability** — No new abstraction; still a plain hex literal, same as
   the rest of the palette. No comment needed (WHY isn't non-obvious).
5. **Completeness** — Confirmed via grep: no remaining `#5b8def` references
   anywhere in the file; both call sites (source-attribute branch and
   systemd-unit-suffix fallback branch) updated.
6. **Performance** — N/A, string literal swap only.
7. **Security** — N/A, no user input or injection surface touched.
8. **API Currency** — N/A, no external library involved.
9. **Build Validation:**
   - `cargo fmt --all -- --check` → passed, no output (clean).
   - `cargo clippy -p vexboard-frontend --target wasm32-unknown-unknown -- -D warnings`
     → failed with `error[E0463]: can't find crate for core` — the
     `wasm32-unknown-unknown` target is not installed in this environment. This
     is a pre-existing environment gap (per CLAUDE.md Resource Constraints:
     "Trunk CLI and the wasm32-unknown-unknown target are not part of a
     standard Rust install"), not a regression introduced by this change. No
     approved safe command in this project's list covers frontend
     compilation/lint, so this could not be independently re-verified via a
     compiler pass. Risk is judged negligible given the change is a single
     hex-literal string substitution with no touched control flow, types, or
     imports.
   - `cargo test -p vexboard-server` and `cargo build --release --bin
     vexboard-server` — not applicable; no server code was touched by this
     change.

## Score Table

| Category                  | Score | Grade |
|----------------------------|-------|-------|
| Specification Compliance   | 100%  | A     |
| Best Practices             | 100%  | A     |
| Functionality               | 100%  | A     |
| Code Quality                | 100%  | A     |
| Security                    | 100%  | A     |
| Performance                 | 100%  | A     |
| Consistency                 | 100%  | A     |
| Build Success               | N/A*  | N/A   |

\* Frontend WASM compilation could not be executed in this environment
(missing `wasm32-unknown-unknown` target); no code-level risk identified for
this literal-value-only change.

**Overall Grade: A (100%, with one unverifiable-by-environment item noted)**

## Result

**PASS** — no critical issues found. The unverified WASM compile step is an
environment gap, not a defect introduced by this change, and does not block
approval.
