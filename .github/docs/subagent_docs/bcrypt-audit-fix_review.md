# bcrypt-audit-fix — Review

## Specification Compliance
Matches spec exactly: `cargo update -p bcrypt` bumped `Cargo.lock` bcrypt entry
from 0.19.1 -> 0.19.2, resolving RUSTSEC-2026-0199. No `Cargo.toml` changes
required (constraint `"0.19"` already permitted 0.19.2). One incidental
transitive shift: `getrandom` 0.4.2 -> 0.3.4 (pulled in via bcrypt's own
dependency resolution, not a direct edit).

## Best Practices
Standard, minimal lockfile-only dependency bump — no code changes required.

## Consistency
No source files touched; only `Cargo.lock`.

## Maintainability
N/A — no new code/abstractions introduced.

## Completeness
RUSTSEC-2026-0199 (bcrypt panic on non-ASCII hash input) is resolved by the bump.
Not addressed (explicitly out of scope per spec, all are warnings not
vulnerabilities, all transitive):
- RUSTSEC-2024-0436 (paste, unmaintained)
- RUSTSEC-2026-0173 (proc-macro-error2, unmaintained)
- RUSTSEC-2026-0190 (anyhow, unsound downcast_mut)

## Performance
No regression expected; bcrypt patch-level bump only.

## Security
Directly fixes the reported medium-severity (5.3) vulnerability.

## API Currency
No API surface change in vexboard-server code; bcrypt 0.19.x API unchanged
between 0.19.1 and 0.19.2 (patch release).

## Build Validation

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Exit 0, no diff |
| `cargo clippy --workspace -- -D warnings` | Exit 0, no warnings |
| `cargo test -p vexboard-server` | Exit 0, 28/28 passed (including login/verify auth tests exercising bcrypt) |
| `cargo build --release --bin vexboard-server` | Exit 0 |
| `cargo audit --ignore RUSTSEC-2023-0071` | Not runnable locally — `cargo-audit` subcommand not installed in this environment. Original report came from CI where it is installed; the fix directly targets the crate/version CI flagged, so it is expected to pass there. Flagging as a gap rather than asserting a result I did not observe. |

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
| Build Success | 100% (audit step unverifiable locally, all other checks pass) | A- |

**Overall Grade: A (98%)**

## Result
PASS
