# bcrypt-audit-fix — Specification

## Current State
- `Cargo.toml` (workspace root, line 23): `bcrypt = "0.19"`
- `crates/vexboard-server/Cargo.toml` (line 29): `bcrypt = { workspace = true }`
- `Cargo.lock`: resolved to `bcrypt 0.19.1`

## Problem
`cargo audit` reports RUSTSEC-2026-0199: panic in `bcrypt::verify` on non-ASCII hash
input, present in bcrypt 0.19.1. Fixed in >=0.19.2. Severity 5.3 (medium).

Additional warnings (not vulnerabilities, out of scope for this fix):
- `paste` 1.0.15 — unmaintained (RUSTSEC-2024-0436), transitive dep
- `proc-macro-error2` 2.0.1 — unmaintained (RUSTSEC-2026-0173), transitive dep
- `anyhow` 1.0.102 — unsound Error::downcast_mut (RUSTSEC-2026-0190), transitive dep

## Proposed Solution
The existing `Cargo.toml` constraint `"0.19"` already permits 0.19.2+ under semver.
No dependency is being added — this is a lockfile update to an already-declared
dependency, so Context7 verification is not required (per Dependency Policy:
"Projects where all dependencies are managed by a lock file with no new additions").

Run `cargo update -p bcrypt` to bump the lockfile to the latest 0.19.x release that
resolves RUSTSEC-2026-0199, then verify via approved build/test/audit commands.

## Implementation Steps
1. `cargo update -p bcrypt`
2. Confirm `Cargo.lock` bcrypt entry is now >=0.19.2
3. Run approved validation commands (fmt, clippy, test, build, audit)

## Dependencies
No new dependencies. Existing `bcrypt` crate, lockfile-only version bump.

## Configuration Changes
None.

## Risks and Mitigations
- Risk: 0.19.2 changes verify() behavior for non-ASCII hashes (that's the fix itself).
  Mitigation: run `cargo test -p vexboard-server` to confirm existing auth tests pass.
- Risk: transitive dependency resolution shifts unrelated crates.
  Mitigation: review `git diff Cargo.lock` to confirm only bcrypt (and its own
  transitive deps, if any) changed.
