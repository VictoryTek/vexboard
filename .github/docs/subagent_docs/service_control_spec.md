# Service Control (Start / Stop / Restart) — Specification

Status: Phase 1 complete, proceeding to Phase 2 implementation.

## 1. Current state analysis

- `probe/uptime.rs` already talks to systemd over D-Bus (via a `#[zbus::proxy]`
  trait, `Connection::system()`, method `list_units`) — read-only.
- `discovery/docker.rs` already talks to Docker/Podman over their Unix
  sockets via `bollard::Docker::connect_with_socket` — read-only
  (`list_containers`).
- `Service.systemd_unit: Option<String>` holds either a real systemd unit
  name, or — per the existing comment in `api/services.rs` — the Docker
  container **name** for services with `discovery_source` `"docker"` /
  `"podman"`. `Service.discovery_source` distinguishes which.
- `config.docker.sockets: Vec<String>` is an ordered list of socket paths;
  `discovery/docker.rs` derives `source` per socket via
  `if socket.contains("podman") { "podman" } else { "docker" }`.
- `middleware::auth::require_admin` already exists and gates every write
  route under `services::admin_router()`.
- `db::audit::insert(pool, actor, action, resource_type, resource_id, detail, ip_addr)`
  already exists and is called from every mutating handler in this file
  (see `update_service` for the exact `actor` extraction idiom).
- `create_service` already has a "fire an immediate re-probe after a
  mutation" pattern (`tokio::spawn` calling `probe::uptime::probe_service`/
  `probe_systemd_unit` right after insert) — reusable here verbatim.
- Feature 1 (this session, prior commit) added `HistoryModal`, opened by
  clicking a service card's sparkline, currently read-only and available to
  every authenticated user.

## 2. Problem definition

Turn VexBoard from a viewer into a control panel: let an admin start, stop,
or restart a tracked service without an SSH round-trip — while keeping the
blast radius of a mis-click bounded, since these are irreversible,
identity-changing actions (stopping the wrong unit takes something down).

## 3. Safety model (the part that needs care)

- **Only services already tracked in VexBoard's own `services` table can be
  controlled.** The client sends a service `id`; the server looks up
  `systemd_unit`/`discovery_source` itself and never accepts a unit or
  container name directly from the request. This reuses the same trust
  boundary the rest of the app already relies on (an admin had to
  explicitly add or claim the service first) rather than introducing a
  second, separate allowlist mechanism — simpler, and no weaker.
- **Admin-only** — same `require_admin` middleware as every other write
  route in this router. No new auth code.
- **Every attempt is audited**, success or failure, via the existing
  `db::audit::insert`, so a bad outcome is traceable.
- **A service with neither a systemd unit nor a container backing it
  (URL-only manual services) is rejected with 400** — there's nothing to
  control.
- **Frontend requires an explicit confirm step before Stop or Restart**
  (not before Start, which is additive/low-risk). Implemented as a
  button-swap ("Stop" → "Confirm Stop?") inside the modal, not a native
  `confirm()` dialog, matching this app's existing UI quality bar.
- Explicitly out of scope: a configurable per-service allowlist/denylist,
  and any change to what discovery surfaces (`server_services_only` etc.
  already restrict what an admin can even add) — the "must already be a
  tracked service" rule is the whole safety mechanism, deliberately kept
  to one rule rather than several overlapping ones.

## 4. Backend design

New module, mirroring the existing `discovery/` split:

```
crates/vexboard-server/src/control/
  mod.rs       // UnitAction enum, module wiring
  systemd.rs   // D-Bus StartUnit/StopUnit/RestartUnit
  docker.rs    // bollard start_container/stop_container/restart_container
```

```rust
// control/mod.rs
pub enum UnitAction { Start, Stop, Restart }
```

```rust
// control/systemd.rs — new proxy trait alongside the existing read-only one
// in probe/uptime.rs (kept separate: different concern, different module)
#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManagerControl {
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

pub async fn control_unit(unit_name: &str, action: UnitAction) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let proxy = SystemdManagerControlProxy::new(&conn).await?;
    match action {
        UnitAction::Start => proxy.start_unit(unit_name, "replace").await?,
        UnitAction::Stop => proxy.stop_unit(unit_name, "replace").await?,
        UnitAction::Restart => proxy.restart_unit(unit_name, "replace").await?,
    };
    Ok(())
}
```

`"replace"` is systemd's standard job mode (same as plain `systemctl
start/stop/restart`). The returned job object path is intentionally
ignored — systemd queues the operation asynchronously; rather than build
job-completion tracking (a `JobRemoved` signal subscription), this reuses
the existing "fire an immediate re-probe" pattern from `create_service` so
the dashboard reflects the outcome within one probe round-trip, which is
proportionate for a homelab dashboard.

```rust
// control/docker.rs
pub async fn control_container(socket: &str, name: &str, action: UnitAction) -> anyhow::Result<()> {
    let docker = bollard::Docker::connect_with_socket(socket, 10, bollard::API_DEFAULT_VERSION)?;
    match action {
        UnitAction::Start => { docker.start_container::<String>(name, None).await?; }
        UnitAction::Stop => { docker.stop_container(name, None).await?; }
        UnitAction::Restart => { docker.restart_container(name, None).await?; }
    }
    Ok(())
}
```

Passing `None` for every options argument (default timeouts) rather than
constructing version-specific option structs — Context7 was checked against
bollard's latest docs (0.20), which use a newer `query_parameters`
builder API than this workspace's pinned `bollard = "0.17"`; `None`
sidesteps that mismatch entirely and `cargo check` is the actual arbiter of
the pinned version's exact signatures.

### New routes (added to `services::admin_router()`, already behind `require_admin`)

```
POST /api/v1/services/{id}/start
POST /api/v1/services/{id}/stop
POST /api/v1/services/{id}/restart
```

One shared handler body parameterized by `UnitAction`, called from three
thin route functions (matching how `utoipa::path` needs one function per
documented operation). Flow:

1. Fetch the `Service` row by id → 404 if missing.
2. If `discovery_source` is `"docker"`/`"podman"`: find the configured
   socket whose derived source matches (same `contains("podman")` rule
   `discovery/docker.rs` already uses) → `control::docker::control_container`.
3. Else if `systemd_unit` is `Some` → `control::systemd::control_unit`.
4. Else → 400, "This service isn't backed by a systemd unit or container."
5. On success: audit-log `service.start`/`service.stop`/`service.restart`
   with `{"display_name": ...}` detail, fire an immediate re-probe
   (identical pattern to `create_service`), return `{"status": "ok"}`.
6. On failure: audit-log the same action with `{"display_name":...,
   "error": ...}` so failed attempts are traceable too, return 502 with
   the error message.

## 5. Frontend design

Extends Feature 1's `HistoryModal` rather than adding a fourth modal shell
— it already opens on a service-card click and has room. `history_target`
grows from `RwSignal<Option<(i64, String)>>` to
`RwSignal<Option<(i64, String, bool)>>` (id, display name, `controllable`
— `svc.systemd_unit.is_some()`, computed where the tuple is already built
in `service_grid.rs`/`group_section.rs`). Threading is otherwise identical
to Feature 1 (no new signal-passing pattern).

Inside the modal, admin-only (existing `CurrentUser` context, same
`is_admin()` idiom used throughout this codebase), shown only when
`controllable` is true: a "Controls" row with Start / Stop / Restart
buttons. Stop and Restart use a local `RwSignal<Option<UnitAction>>`
"pending confirmation" state — first click swaps the button to "Confirm
Stop?"/"Confirm Restart?" in the danger color; a second click within the
same modal session actually fires the request. Start fires immediately.
Each action calls the new endpoint, shows a small inline status message
(pending/success/error — every request result is surfaced, none
discarded), and triggers `summary.set(None)` + a refetch so the heartbeat
bar/uptime figures pick up the new state shortly after.

New CSS: one `.btn-danger` rule (mirrors the existing `.btn-primary`/
`.btn-secondary` shape, using `--color-danger`/`--color-danger-dim`) plus a
small `.history-controls` row layout — both appended to the same
`.history-*` block Feature 1 added.

## 6. Dependencies

None new — `bollard` and `zbus` are already workspace dependencies, already
used for read operations. Verified via Context7 (see §4) that the pinned
bollard version predates the `query_parameters` builder refactor shown in
current upstream docs; adapted accordingly rather than copying the newer
API verbatim.

## 7. Files touched

Backend: `control/mod.rs` (new), `control/systemd.rs` (new),
`control/docker.rs` (new), `main.rs` (register `mod control;`),
`api/services.rs` (3 routes + handler), `api/openapi.rs` (register 3 paths).
Frontend: `components/history_modal.rs`, `pages/dashboard/service_grid.rs`,
`pages/dashboard/group_section.rs`, `style/main.css`.

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Admin stops a unit the server itself depends on (e.g. its own systemd unit, if self-added) | Same self-inflicted-risk shape as any control panel (Kuma, Cockpit, Portainer); the "must be a tracked service" rule means it requires deliberate action, not an accident |
| bollard 0.17's exact option-struct shape differs from what Context7's newer docs show | Every call passes `None` for options, sidestepping struct-shape questions entirely; `cargo check`/clippy is the real verification gate |
| A stop/restart appears to hang because systemd jobs are async | Immediate re-probe after firing gives feedback within one probe cycle without building job-completion tracking |
| Failed control attempts go unnoticed | Every attempt — success or failure — writes to the audit log; the frontend surfaces the error inline rather than discarding it |

## 9. Approved validation commands

Same as established: `cargo fmt --all -- --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test -p vexboard-server`,
`cargo build --release --bin vexboard-server`, `scripts/preflight.ps1`.
