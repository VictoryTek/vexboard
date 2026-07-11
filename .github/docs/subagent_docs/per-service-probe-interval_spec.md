# BUG-5 — Per-Service `probe_interval` Ignored — Spec

## Current State Analysis

`start_probe_loop` (`crates/vexboard-server/src/probe/mod.rs:12-59`) runs an infinite loop that,
every `config.default_interval_secs` (30s by default, `config/default.toml:120`), fetches every
service with `probe_enabled = 1` and unconditionally spawns a probe task for each one. The
per-service `probe_interval` column (`Service.probe_interval: i64`, non-null, DB default `30`,
`crates/vexboard-server/src/db/migrations/001_init.sql:23`) — which flows through the schema,
model, DTOs, OpenAPI spec, and frontend edit UI — is fetched into the `Service` struct but never
read anywhere in the scheduler. Every enabled service is probed at the same global cadence
regardless of its configured `probe_interval`.

## Problem Definition

`probe_interval` is dead data as far as scheduling is concerned; a service configured for a
5-minute interval is probed every 30 seconds like everything else, wasting probe cycles and
ignoring explicit user configuration.

## Proposed Solution

Decouple the scheduler's wake-up cadence from the probing cadence:

- Introduce a short, fixed base tick (`TICK_SECS`, 5 seconds) at which the loop wakes and
  re-evaluates which services are due.
- Track `last_probed: HashMap<i64, Instant>` across loop iterations (owned by
  `start_probe_loop`'s local scope, mutated only from the single loop task — no locking needed).
- On each tick, for every enabled service, probe it (and record `Instant::now()` in the map) only
  if `last_probed.get(&svc.id)` is absent or its `elapsed()` is `>= Duration::from_secs(svc.
  probe_interval.max(1) as u64)` (the `max(1)` guards against a non-positive `probe_interval`
  value being cast to `u64` and producing a huge duration, or a `0` triggering back-to-back
  probing on every 5s tick — either way `max(1)` keeps the comparison well-defined without
  changing the loop's own tick cadence).
- Prune `last_probed` entries for service IDs no longer present in the current fetch each tick,
  since the map is new state introduced by this fix — without pruning, deleted services' entries
  would accumulate indefinitely (the same unbounded-growth class of issue as BUG-30, but here it
  would be *newly introduced* by this change, so it is fixed inline rather than left as a
  separate follow-up).

`config.default_interval_secs` becomes unused by this change (each service's own `probe_interval`
now governs its cadence; the loop's own wake frequency is the new fixed `TICK_SECS`). Leaving
the config field in place is intentional — removing it would be a breaking config change out of
scope for this bug fix, and the field remains valid, documented configuration surface (harmless
if unread; verified not to trigger a compiler/clippy warning in the Approved build commands: pub
struct fields populated via `serde::Deserialize` are not flagged as dead code by rustc/clippy
since the derive macro constructs the field as part of deserialization).

## Implementation Steps

In `crates/vexboard-server/src/probe/mod.rs`:

1. Add imports: `std::collections::HashMap`, `std::time::Instant`.
2. Replace:
   ```rust
   let interval = Duration::from_secs(config.default_interval_secs);

   loop {
   ```
   with:
   ```rust
   const TICK_SECS: u64 = 5;
   let tick = Duration::from_secs(TICK_SECS);
   let mut last_probed: HashMap<i64, Instant> = HashMap::new();

   loop {
   ```
3. Inside the `if let Ok(services) = services` block, before the `for svc in services` loop,
   prune stale entries and compute due services:
   ```rust
   let current_ids: std::collections::HashSet<i64> = services.iter().map(|s| s.id).collect();
   last_probed.retain(|id, _| current_ids.contains(id));

   for svc in services {
       let due = last_probed
           .get(&svc.id)
           .is_none_or(|t| t.elapsed() >= Duration::from_secs(svc.probe_interval.max(1) as u64));
       if !due {
           continue;
       }
       last_probed.insert(svc.id, Instant::now());

       let db = db.clone();
       // ...existing spawn body unchanged...
   }
   ```
4. Replace the final `tokio::time::sleep(interval).await;` with `tokio::time::sleep(tick).await;`.

No changes to `uptime::probe_service` / `uptime::probe_systemd_unit`, DTOs, or the frontend —
this is purely a scheduling-loop fix.

## Dependencies

None new — `std::collections::{HashMap, HashSet}` and `std::time::Instant` are all stable std.

## Configuration Changes

None required. `config.default_interval_secs` remains a valid (now unused-by-the-scheduler)
config key; no migration needed.

## Risks and Mitigations

- **Risk:** `svc.probe_interval` is `i64` and could theoretically be negative (no DB CHECK
  constraint enforces positivity) if set via a future direct-DB or crafted-API write.
  **Mitigation:** `.max(1)` before the `as u64` cast prevents the value from being negative
  going into the cast, so the resulting `Duration` is always well-defined and at least 1 second.
- **Risk:** `HashMap::retain` + a fresh `HashSet` allocation every tick (every 5s) adds minor
  overhead proportional to the number of enabled services.
  **Mitigation:** Negligible for a self-hosted dashboard's expected service counts (tens, not
  thousands); this is the same order of allocation the existing code already performs every tick
  via the `Vec<Service>` fetch itself.
- **Risk:** Services probed less frequently than before (if `probe_interval` > old global
  `default_interval_secs`) will show slightly staler status between checks.
  **Mitigation:** This is the intended, correct behavior — the user explicitly configured that
  interval; this bug fix makes that configuration take effect for the first time.

## Test Plan

`cargo test -p vexboard-server` — `start_probe_loop` is an infinite background loop with no
existing test coverage (it's spawned once at server startup, not called per-request), so no
existing test exercises it directly. No new test is added: testing an infinite loop's
timing-dependent due/not-due branching would require either sleeping in the test (slow, flaky)
or refactoring the due-check into a separately-testable pure function, which is a larger
restructuring than this targeted scheduling fix warrants. The change is verified via
`cargo build`/`clippy` type-checking (the new `HashMap`/`HashSet`/`Instant` usage, the `.max(1)
as u64` cast, and `Option::is_none_or` all compile cleanly) plus the existing full test suite
continuing to pass unaffected (no test touches probe scheduling).
