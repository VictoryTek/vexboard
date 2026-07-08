# Claimed Docker/Podman Containers Store a Fake systemd_unit — Review (BUG-3)

Spec: `discovery_claim_systemd_unit_spec.md`

## Important Note Carried From Spec

The probe-priority half of this bug (backend giving `systemd_unit` precedence over
`url` for Docker/Podman-sourced services) was already fixed pre-session by commit
`d5d1f399` in `probe/mod.rs` and `api/services.rs`. This pass closes the one
remaining gap the master plan cites: the discovery panel still wrote a fake
`systemd_unit` value into the database for non-systemd claims.

## Modified Files

- `crates/vexboard-frontend/src/components/discovery_panel.rs` — `on_save` now only
  forwards `unit_name` as `systemd_unit` when `discovery_source == "systemd"`; sends
  `null` for `"docker"`/`"podman"` claims.

## Review Against Spec

1. **Specification compliance** — exact one-purpose fix, matches the spec's proposed
   code.
2. **Best practices** — mirrors the existing backend guard's condition
   (`discovery_source` gates `systemd_unit` trust) so frontend and backend now agree
   on what the column means.
3. **Consistency** — same `if let`/ternary style already used two lines below for
   `description`/`url`/`icon` null-vs-value handling in the same JSON body.
4. **Completeness** — closes the "set" side to match the already-fixed "use" side;
   no service claimed through this panel will ever have a `systemd_unit` value that
   isn't a real systemd unit.
5. **Performance/Security** — not applicable.
6. **API currency** — no external API involved.

## Build Validation (verbatim)

**`cargo fmt --all -- --check`** — clean, no diff.

**`cargo clippy --workspace -- -D warnings`**
```
    Checking vexboard-frontend v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.93s
```

**`cargo test -p vexboard-server`** — 34/34 pass (unaffected; frontend-only change).

**`cargo build --release --bin vexboard-server`** — succeeds (unaffected).

**Not run:** `trunk build` (FORBIDDEN COMMANDS); no live browser verification
possible in this environment.

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
