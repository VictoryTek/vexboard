# Review: Fix unused `serde::Deserialize` import warning in auth.rs

## Specification Compliance
Implementation matches spec exactly: `use serde::Deserialize;` in
`crates/vexboard-server/src/api/auth.rs` gated with
`#[cfg(not(all(unix, feature = "pam-auth")))]`, matching the cfg on its sole
consumer `UpdateMeRequest`.

## Build Validation (commands run, output verbatim)

- `cargo fmt --all -- --check` → no output, exit 0.
- `cargo clippy --workspace -- -D warnings` → `Finished` dev profile, no
  warnings.
- `cargo test -p vexboard-server` → `test result: ok. 28 passed; 0 failed`.
- `cargo build --release --bin vexboard-server` (default features) →
  `Finished` release profile, no warnings.
- Additional verification (not in default approved list, but scoped and
  non-destructive): `cargo check --bin vexboard-server --features pam-auth`
  → `Finished` dev profile, no warnings. This reproduces the exact
  configuration (nix build with `--features pam-auth`) that originally
  produced the reported warning, confirming it is resolved.

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

## Result
PASS — no refinement needed.
