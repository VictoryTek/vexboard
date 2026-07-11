# BUG-5 — Per-Service `probe_interval` Ignored — Review

## Summary

Implementation matches spec exactly in `crates/vexboard-server/src/probe/mod.rs`:

- The loop's sleep interval decoupled from `config.default_interval_secs` and replaced with a
  fixed `TICK_SECS = 5` constant, documented with a comment explaining why (tick cadence vs.
  per-service probe cadence are now separate concerns).
- `last_probed: HashMap<i64, Instant>` tracks the last probe time per service across loop
  iterations.
- Each tick, `last_probed` is pruned to only IDs present in the current fetch (prevents the
  newly-introduced map from growing unboundedly for deleted services).
- A service is only spawned for probing when `last_probed.get(&svc.id).is_none_or(|t| t.elapsed()
  >= Duration::from_secs(svc.probe_interval.max(1) as u64))` — i.e., never probed yet, or its own
  `probe_interval` has elapsed. `.max(1)` guards the `i64 → u64` cast against non-positive values.
- `last_probed.insert(svc.id, Instant::now())` is recorded immediately when a service is judged
  due, before the async probe task is spawned (correct — avoids re-triggering on the next tick
  while the previous probe is still in flight).
- No changes to `uptime::probe_service`/`uptime::probe_systemd_unit`, DTOs, or frontend, matching
  the spec's stated scope.

## Build & Test Results (verbatim)

`cargo fmt --all -- --check` — exit 0, no output (clean).

`cargo clippy --workspace -- -D warnings`:
```
    Checking vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.96s
```
Exit 0, no warnings — confirms `config.default_interval_secs` becoming unused-by-the-scheduler
does not trigger a dead-code lint (it remains a live, `Deserialize`-populated `pub` config field),
as anticipated in the spec's risk analysis.

`cargo test -p vexboard-server`:
```
running 34 tests
...
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```
Exit 0.

`cargo build --release --bin vexboard-server`:
```
    Compiling vexboard-server v0.1.1 (/home/nimda/Projects/vexboard/crates/vexboard-server)
    Finished `release` profile [optimized] target(s) in 10.91s
```
Exit 0.

## Review Against Criteria

1. **Specification Compliance** — exact match to spec.
2. **Best Practices** — `Option::is_none_or` (stable since Rust 1.82, confirmed available:
   `rustc 1.96.1` in this toolchain) is the idiomatic combinator for this "absent-or-satisfies"
   check, avoiding a more verbose `match`.
3. **Consistency** — matches the existing pattern of cloning `db`/`tx` per-service before
   `tokio::spawn`; no new architectural style introduced.
4. **Maintainability** — the tick-vs-cadence distinction is explained in a short comment at the
   point of definition; the due-check is a single small closure, easy to follow.
5. **Completeness** — fully resolves BUG-5: `probe_interval` now genuinely governs per-service
   cadence instead of being dead data.
6. **Performance** — the `HashSet<i64>` allocation and `retain` pass every 5s is proportional to
   the (small, self-hosted-scale) enabled-service count; negligible next to the existing
   `Vec<Service>` fetch already performed every tick.
7. **Security** — none; scheduling-only change, no new attack surface.
8. **API Currency** — no external API involved; `is_none_or` is current stable std, not
   deprecated.
9. **Build Validation** — all four approved commands run clean (see above).

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

- Build result: PASS (fmt, clippy, tests, release build all clean)
- **PASS**
