# Review: Discovered list not updated reactively after adding a service

## Spec Reference
`.github/docs/subagent_docs/discovered_list_stale_after_add_spec.md`

## Change Summary
In `crates/vexboard-server/src/api/services.rs`, `create_service`'s success branch now synchronously removes the matching entry from `state.discoveries` (matched on `discovery_source`/`systemd_unit`) immediately after insert succeeds, before the HTTP response is returned — mirroring the existing `dismiss_unit` pattern in `crates/vexboard-server/src/discovery/mod.rs`.

## Findings

1. **Specification Compliance** — Implementation matches the spec exactly: guarded on `Some(..)` for both `discovery_source` and `systemd_unit`, retains non-matching entries, drops the lock before continuing. No frontend changes were needed, as predicted.
2. **Best Practices** — Follows the exact idiom already used by `dismiss_unit` (write lock, `retain`, explicit `drop`). Consistent with Rust/Axum/sqlx conventions in this file.
3. **Consistency** — Matches surrounding code style (reference comparison via `&u.source == source`), placed logically right after `new_id` is obtained and before the background probe spawn.
4. **Maintainability** — Small, self-contained, four-line change; no new abstractions introduced.
5. **Completeness** — Addresses the root cause identified in research: `create_service` previously never touched `state.discoveries`, causing the frontend's `units.refetch()` to race the fire-and-forget `/discovery/refresh` background scan.
6. **Performance** — Negligible; one extra write-lock acquisition and a linear `retain` over an in-memory `Vec` that is already small (unclaimed discovered units).
7. **Security** — No new attack surface; uses existing authenticated route and existing state.
8. **API Currency** — No external library involved; N/A for Context7.
9. **Build Validation** — see below, all commands passed.

## Build Validation (Approved Commands)

- `cargo fmt --all -- --check` → **PASS**, no output/diff.
- `cargo clippy --workspace -- -D warnings` → **PASS**, `Finished` with zero warnings (only `vexboard-server` was compiled; no native WASM-target issue encountered).
- `cargo test -p vexboard-server` → **PASS**, 47 passed; 0 failed; 0 ignored. No SIGSEGV observed.
- `cargo build --release --bin vexboard-server` → **PASS**, `Finished release [optimized] target(s)`.

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
**PASS** — no refinement needed.
