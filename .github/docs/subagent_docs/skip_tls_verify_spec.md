# skip_tls_verify — Spec

## Current State Analysis

Bug report: the Proxmox service card shows "Down" even though the Proxmox server is genuinely reachable.

Root cause, confirmed by code inspection:
- `crates/vexboard-server/src/main.rs:196-199` builds a single process-wide `reqwest::Client` used for every HTTP probe:
  ```rust
  let probe_client = reqwest::Client::builder()
      .timeout(std::time::Duration::from_secs(config.probe.timeout_secs))
      .danger_accept_invalid_certs(false)
      .build()?;
  ```
- `danger_accept_invalid_certs(false)` enforces strict TLS certificate validation with no exceptions.
- Proxmox VE's web UI (port 8006) uses a self-signed certificate by default. Any HTTPS probe against it fails the TLS handshake in `crates/vexboard-server/src/probe/uptime.rs::probe_service` (both the `HEAD` at line 63 and the `GET` fallback at line 80 return `Err`), so the service is recorded as `"down"` (uptime.rs:93-96) even though a browser (which lets a user click through the cert warning) reaches it fine.
- There is no Proxmox-specific code anywhere in the repo (confirmed via full-repo grep — only two unrelated icon-catalog entries).
- There is no per-service TLS setting; `Service.url` (`crates/vexboard-server/src/db/models.rs:21`) is a single free-form URL string with no companion TLS-verification flag.

Prior art check (user-requested): Uptime Kuma and Homepage — the two most comparable self-hosted dashboard/uptime tools — both solve this with a **per-monitor/per-service "ignore TLS/SSL errors" toggle**, not a global insecure switch. This spec follows that established pattern.

## Problem Definition

Add a per-service opt-in flag, `skip_tls_verify`, that lets a user mark a specific service (e.g. Proxmox) as having a self-signed/untrusted certificate. When set, the HTTPS probe for that service accepts any certificate; all other services keep strict verification by default. No behavior change for existing services (default `false`).

## Proposed Solution Architecture

1. **DB**: add `skip_tls_verify BOOLEAN NOT NULL DEFAULT 0` to `services`, via a new idempotent migration `009_skip_tls_verify.sql`, following the existing `ALTER TABLE ... ADD COLUMN` + `pragma_table_info` guard pattern used for `discovery_source`/`role`/`color`.
2. **Backend model/DTOs**: add `skip_tls_verify: bool` to `Service`, and `skip_tls_verify: Option<bool>` to `CreateService`/`UpdateService` (mirroring `probe_enabled`).
3. **Backend HTTP clients**: build a second shared `reqwest::Client` in `main.rs` with `.danger_accept_invalid_certs(true)`, alongside the existing strict client. Both are passed down to the probe scheduler; `probe::start_probe_loop` and the immediate-probe-on-create path in `api/services.rs::create_service` pick the insecure client when `svc.skip_tls_verify` is true, else the strict client. `probe::uptime::probe_service`'s signature is unchanged (it already just takes `&reqwest::Client` — the caller decides which one).
4. **API layer**: thread `skip_tls_verify` through every SQL column list and bind (`list_services`, `create_service` INSERT + its post-insert immediate-probe SELECT, `update_service` SELECT + UPDATE, `probe::mod.rs`'s scheduler SELECT).
5. **Frontend**: add `skip_tls_verify: bool` to `EditFormData`, `ServiceResponse`, and thread it through the two mapping sites (`service_grid.rs`, `group_section.rs`) and the four JSON-body construction sites (`modals.rs` ×2, `discovery_panel.rs` ×2). Add an actual checkbox to `EditModal` (`modal_edit.rs`) — this is the **first** boolean toggle exposed in that form, so a small new UI element is introduced: a `<input type="checkbox">` bound to a live signal via `prop:checked` / `on:change` reading `event_target_checked`, labeled "Skip TLS certificate verification (self-signed certs)". This is a live, user-editable field (unlike the pass-through `probe_enabled`/`probe_interval`, which have no UI today and are out of scope — not touched by this change).

## Implementation Steps

Backend:
1. `crates/vexboard-server/src/db/migrations/009_skip_tls_verify.sql` — `ALTER TABLE services ADD COLUMN skip_tls_verify BOOLEAN NOT NULL DEFAULT 0;`
2. `crates/vexboard-server/src/db/mod.rs::run_migrations` — add idempotency guard block (mirrors `has_discovery_source`/`has_role` pattern) before the final `tracing::info!`.
3. `crates/vexboard-server/src/db/models.rs` — add field to `Service` (after `probe_interval`), `CreateService`, `UpdateService`.
4. `crates/vexboard-server/src/api/services.rs`:
   - `list_services` SELECT column list (line 75-77)
   - `create_service` INSERT column list + bind (line 250-266), post-insert probe SELECT (line 281-284)
   - `update_service` SELECT (line 378-380), `let skip_tls_verify = payload.skip_tls_verify.unwrap_or(existing.skip_tls_verify);`, UPDATE column list + bind (line 439-457)
5. `crates/vexboard-server/src/probe/mod.rs` — scheduler SELECT column list (line 30-33); `start_probe_loop` signature gains a second `reqwest::Client` param (e.g. `insecure_client`); dispatch picks client based on `svc.skip_tls_verify`.
6. `crates/vexboard-server/src/main.rs`:
   - Build `probe_client_insecure` alongside `probe_client` (same timeout, `.danger_accept_invalid_certs(true)`).
   - Pass both into `probe::start_probe_loop(...)` (line 232-234).
   - `create_service`'s immediate-probe `tokio::spawn` block in `api/services.rs` needs access to the right client too — simplest: add `probe_client_insecure: reqwest::Client` to `AppState`, and in `create_service` pick `if svc.skip_tls_verify { &state.probe_client_insecure } else { &state.probe_client }` (clone whichever is needed into the spawned task).

Frontend:
7. `crates/vexboard-frontend/src/components/modal_edit.rs` — add field to `EditFormData`, default `false`, new signal, checkbox UI block (placed after the Icon block, before the Group selector), include `skip_tls_verify: skip_tls_verify.get()` in `on_save.run(...)`.
8. `crates/vexboard-frontend/src/pages/dashboard/mod.rs` — add field to `ServiceResponse`.
9. `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs` and `group_section.rs` — map `svc.skip_tls_verify` into `EditFormData`.
10. `crates/vexboard-frontend/src/pages/dashboard/modals.rs` — add `"skip_tls_verify": data.skip_tls_verify` to both JSON bodies (add + edit).
11. `crates/vexboard-frontend/src/components/discovery_panel.rs` — add to POST body and to the discovered-unit `EditFormData` init (default `false`).

## Dependencies

None new — `reqwest` already provides `danger_accept_invalid_certs`; no new crate, no Context7 lookup needed (internal-only change, per CLAUDE.md's Context7 exemption for changes with no new dependencies).

## Configuration Changes

None to `config/default.toml` — this is a per-service DB-backed flag, not a global config option (consistent with Uptime Kuma/Homepage precedent and with how `probe_enabled`/`probe_interval` are already modeled).

## Risks and Mitigations

- **Risk**: accidentally widening TLS bypass to all services. **Mitigation**: default `false` at both the DB column and DTO level; only services explicitly flagged use the insecure client.
- **Risk**: `AppState` clone cost of carrying two `reqwest::Client`s. **Mitigation**: `reqwest::Client` is `Arc`-backed internally and cheap to clone; this matches the existing pattern for `probe_client`.
- **Risk**: migration column name collision on re-run. **Mitigation**: guarded by `pragma_table_info` count check, matching existing migrations 003/004.
- **Risk**: forgetting one of the several SELECT column lists (services table columns are listed manually in 4+ places, not derived). **Mitigation**: implementation phase must grep for `probe_interval` across the backend to catch every column-list site before finishing.
