# Uptime History & Incident Timeline — Specification

Status: Phase 1 complete, proceeding to Phase 2 implementation.

## 1. Current state analysis

- `probe_results` (`db/migrations/001_init.sql:30-36`) already stores every
  probe check: `service_id`, `status` (`up`/`down`/`unknown`), `latency_ms`,
  `checked_at`.
- Both `probe::uptime::probe_service` and `probe::uptime::probe_systemd_unit`
  (`probe/uptime.rs`) insert a row, then trim with
  `DELETE ... WHERE service_id = ? AND id NOT IN (SELECT id ... ORDER BY
  checked_at DESC LIMIT ?)`, bound to `config.probe.max_history` (default
  100, `config/default.toml:126`).
- **This means retention is row-count-based, not time-based.** A service
  probed every 30s retains ~50 minutes of history; a service probed every
  5 minutes retains ~8 hours. Two services' "uptime %" figures are not
  comparable, and neither reaches anywhere near 24h.
- `GET /api/v1/services/{id}/history?limit=N` (`api/services.rs:161-189`,
  N clamped 1-100) returns raw points, oldest-first. The dashboard card
  (`components/service_card.rs::history_strip`) calls this with `limit=100`
  and renders a latency sparkline + a single "uptime %" computed as
  `up_count / total_count` over whatever rows come back — i.e. over the
  same too-short, inconsistent window described above.
- No incident concept exists anywhere — there is no query or endpoint that
  turns a run of down/unknown checks into "down from X to Y for Z."
- Nothing outside the raw sparkline strip is clickable; there is no detail
  surface for a single service's history.

## 2. Problem definition

Ship what the pitch promised: real 24h/7d/30d uptime percentages, a
heartbeat-style view of recent checks, and an incident list with durations
— without breaking the existing card sparkline, and without front-running
Feature 4 (service detail view), which will later want a home for this
same data alongside logs.

## 3. Proposed solution

### 3a. Backend — time-windowed retention (replaces row-count retention)

Rename `ProbeConfig::max_history: u64` to `history_retention_days: u64`
(default 30). The prune query in both probe functions changes from
"keep the last N rows" to "delete rows older than N days":

```rust
let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::days(retention_days as i64);
sqlx::query("DELETE FROM probe_results WHERE service_id = ? AND checked_at < ?")
    .bind(svc.id)
    .bind(cutoff)
    .execute(db).await
```

This is simpler than the current `NOT IN (SELECT ... LIMIT ?)` subquery, not
just more correct. Both `probe_service` and `probe_systemd_unit` take
`retention_days: u64` in place of `max_history: u64`; the one call site in
`probe/mod.rs`'s scheduler loop and the two in `api/services.rs`'s
immediate-reprobe-on-create path pass `config.history_retention_days`.

Renamed everywhere the old key appears: `config.rs`, `config/default.toml`,
`tests.rs` fixture, `README.md`'s architecture-flow mention. The Nix module
(`nix/module.nix`) needs no change — `settings` is a freeform TOML
passthrough with no hardcoded field for this key. Existing deployments that
never overrode `probe.max_history` are unaffected (the bundled
`config/default.toml` supplies `history_retention_days` as the base layer);
an old explicit `max_history` override in a user's own config file or Nix
`settings` block will be silently ignored by the `config` crate (no
`deny_unknown_fields`) rather than erroring — acceptable for an internal
tuning knob that isn't exposed in the Settings UI today.

### 3b. Backend — uptime summary endpoint

New route: `GET /api/v1/services/{id}/uptime`. One query, fetching the most
recent `MAX_SUMMARY_ROWS` (20,000 — a safety cap independent of the
retention window, in case a very short probe interval outpaces the prune
cycle) rows for the service, most-recent-first, reversed to chronological
order:

```rust
const MAX_SUMMARY_ROWS: i64 = 20_000;
```

From that one fetched, ordered set, compute in Rust (not SQL — the
"gaps and islands" grouping needed for incidents is a straightforward
single-pass scan in Rust and a nested-CTE SQL query would be harder to
read, test, and maintain for no real performance benefit at homelab scale):

- `uptime_24h` / `uptime_7d` / `uptime_30d: Option<f64>` — percentage of
  `status == "up"` rows whose `checked_at` falls within each window;
  `None` when the window has zero rows (service too new / never probed in
  that window) rather than a misleading `0.0`.
- `heartbeats: Vec<ProbeHistoryPoint>` — the last 50 rows of the same
  fetched set (no second query).
- `incidents: Vec<Incident>` — every maximal run of consecutive non-"up"
  rows, most-recent-first:
  ```rust
  pub struct Incident {
      pub status: String,               // last non-"up" status seen in the run
      pub started_at: NaiveDateTime,
      pub ended_at: Option<NaiveDateTime>,  // None = still ongoing
      pub duration_secs: i64,               // now - started_at while ongoing
      pub check_count: i64,
  }
  ```

Derivation lives in `probe/uptime.rs` as two pure, independently unit-tested
functions (matching the existing `discovery::systemd::tests` convention of
colocating parsing/derivation logic with its own module and tests):

```rust
pub fn compute_uptime_summary(rows: &[ProbeHistoryPoint], now: NaiveDateTime) -> UptimeSummary
fn derive_incidents(rows: &[ProbeHistoryPoint], now: NaiveDateTime) -> Vec<Incident>
```

`UptimeSummary` and `Incident` are added to `db/models.rs` (Serialize +
utoipa::ToSchema, matching `ProbeHistoryPoint`) and registered in
`api/openapi.rs`'s schema list.

### 3c. Frontend — history modal

The existing sparkline strip on each service card becomes clickable and
opens a new `HistoryModal` (`components/history_modal.rs`), following the
exact modal convention already used by `EditModal`/`GroupsModal`
(full-screen overlay + blurred backdrop + centered panel, inline styles,
no new CSS-class system) and the exact `RwSignal<Option<(i64, ...)>>`
target-passing convention `edit_target`/`edit_link_target` already use in
`pages/dashboard/mod.rs`.

`history_target: RwSignal<Option<(i64, String)>>` (id, display name) is
created once in `DashboardPage`, threaded through to `ServiceGrid` and
`GroupSection` (both already receive `edit_target` the same way) so the
click handler is available regardless of sort mode, and rendered via
`DashboardModals`. Opening it is **not** admin-gated — viewing uptime
history is a read action available to every authenticated user, unlike
edit/delete.

Modal contents, single fetch to the new endpoint on open:
- Service name (from the target tuple, no extra round trip)
- Three stat tiles: 24h / 7d / 30d uptime (`—` when `None`)
- A heartbeat bar: 50 colored segments (up=green/`--color-success`,
  down=red/`--color-danger`, unknown=grey/`--color-text-muted`), each with a
  `title=` tooltip showing status + timestamp — the discrete-block idiom the
  reference product uses, distinct from the card's existing continuous
  latency sparkline (which is untouched by this feature)
- Incident list: status-colored dot, "started_at to ended_at" or "ongoing",
  human duration, check count. Empty state: "No incidents in the retained
  history."

New CSS block in `main.css`, scoped to this feature (`.history-*` classes),
reusing existing `--color-*` tokens exclusively — same approach as the
settings facelift's page-scoped block.

## 4. What this explicitly does NOT include (deferred)

- A full per-service detail page/route — that's Feature 4's job. This modal
  is intentionally self-contained so Feature 4 can later embed or replace
  it without entangling this change with routing work.
- Any change to the existing card sparkline/`% uptime` label or its
  `/history?limit=N` endpoint — untouched, still valid post-migration since
  it only asks for "the most recent N rows," which remains meaningful once
  the backing table holds a time window instead of a fixed row count.
- Configurable retention via the Settings UI (the mockup showed a "Keep
  history for" selector) — the Settings page facelift only migrated
  existing controls; exposing `history_retention_days` there is a natural
  follow-up once the generalized settings API (spec step 4 from the
  facelift work) exists, not blocking this feature.

## 5. Dependencies

None new — `chrono` (already a workspace dependency, already used via
`NaiveDateTime`/`Utc::now()` elsewhere) covers the duration arithmetic. No
Context7 lookup required.

## 6. Files touched

Backend: `config.rs`, `config/default.toml`, `tests.rs`, `README.md`,
`probe/uptime.rs`, `probe/mod.rs`, `api/services.rs`, `db/models.rs`,
`api/openapi.rs`.
Frontend: `components/service_card.rs`, `components/history_modal.rs` (new),
`pages/dashboard/mod.rs`, `pages/dashboard/service_grid.rs`,
`pages/dashboard/group_section.rs`, `pages/dashboard/modals.rs`,
`style/main.css`.

## 7. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Config key rename silently drops a customized `max_history` | Default (30d) covers the base layer; the old key isn't Settings-UI-exposed today, so real-world impact is limited to anyone who hand-edited `config.toml` |
| Pathological short probe interval leads to a huge per-service row count | `MAX_SUMMARY_ROWS = 20_000` caps the summary query independent of retention; nightly prune still bounds table growth by age |
| Incident derivation logic has an off-by-one at window boundaries | Pure function, unit-tested directly with synthetic `ProbeHistoryPoint` sequences before wiring into the handler |
| New modal fetch adds a request per open | Single request, only on explicit user click — no background polling added |

## 8. Approved validation commands

Same as established: `cargo fmt --all -- --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test -p vexboard-server`,
`cargo build --release --bin vexboard-server`, `scripts/preflight.ps1`.
