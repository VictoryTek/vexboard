# Service Detail: Live Logs — Specification

Status: Phase 1 complete, proceeding to Phase 2 implementation.

## 1. Current state analysis

- Feature 1 already built a per-service detail surface: `HistoryModal`,
  opened by clicking a service card, showing uptime %, a heartbeat bar,
  and incidents. Feature 2 added admin-only Start/Stop/Restart controls to
  the same modal. Feature 1's spec explicitly called this out as the
  future home for logs: *"self-contained so Feature 4 can later embed or
  replace it without entangling this change with routing work."*
- No log-reading code exists anywhere in the backend today.
- Two existing precedents this feature reuses directly:
  - SSE streaming: `api/metrics.rs::metrics_stream` and
    `api/services.rs::stream_service_events` both already return
    `Sse<impl Stream<Item = Result<Event, Infallible>>>` off a
    `tokio::sync::broadcast` channel.
  - Subprocess execution: `discovery/systemd.rs::query_container_port`
    already shells out via `tokio::process::Command` (to `docker port`/
    `podman port`) and handles a non-zero exit cleanly.
- `control/` (Feature 2) already has the exact systemd-vs-container split
  this feature needs: D-Bus for systemd units, bollard for Docker/Podman.

## 2. Problem definition

"It's red, why?" is the question that follows every alert, and today the
only answer is SSH + `journalctl`/`docker logs`. Give the admin the last
line of investigation from inside the same modal they already use to see
status and fire a restart.

## 3. Scope

**In scope:**

- One new SSE endpoint, `GET /api/v1/services/{id}/logs/stream`, admin-only
  (stricter than Feature 1's history view — log output is arbitrary text a
  service prints, which occasionally includes things like stack traces,
  connection strings, or other output an admin wouldn't want a viewer
  role seeing, unlike a numeric uptime percentage).
- systemd units: `journalctl -u <unit> -n 50 -f --no-pager -o short-iso`,
  spawned with `.kill_on_drop(true)` so an abandoned connection can't leak
  the child process — the same subprocess pattern already used for
  `docker port`, just long-lived and piped instead of one-shot.
- Docker/Podman: bollard's native `Docker::logs(...)` streaming API
  (`follow: true`, `stdout: true`, `stderr: true`, `tail: "50"`) — no CLI
  subprocess needed here, matching how `control/docker.rs` already talks
  to the daemon directly rather than shelling out.
- Manual (URL-only) services: rejected the same way Feature 2 rejects
  control actions on them — nothing to tail.
- Frontend: a "Logs" toggle inside the existing `HistoryModal`. Off by
  default — opening the modal does **not** start a log stream; only
  clicking "Logs" does, and closing it (or the modal) closes the
  `EventSource` explicitly so the server-side process/stream is released
  promptly rather than waiting for a timeout.

**Explicitly deferred**, narrowing the original four-part pitch
("logs, alongside per-service resource stats — restarts, memory/CPU,
image tag, ports") to just the logs half, the same way every prior
feature shipped a focused slice:

- Per-unit/per-container resource stats (memory, CPU, image tag, ports) —
  a separate, additive panel with no dependency on the logs work here.
- Log search/filter/download.
- Historical scrollback beyond the initial `-n 50`/`tail: "50"` window —
  no pagination; this is a live tail, not a log archive.

## 4. Design

### 4a. Backend

```rust
// api/services.rs — added to admin_router(), alongside start/stop/restart
.route("/{id}/logs/stream", get(service_logs_stream))
```

Handler flow mirrors `control_service`'s service lookup exactly (server
resolves `systemd_unit`/`discovery_source` from the id; the client never
supplies a unit/container name), then branches:

```rust
async fn service_logs_stream(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    // fetch Service by id — 404 if missing, 400 if neither unit nor container
    let stream: Result<BoxedLogStream, String> = if is_container {
        docker_log_stream(&socket, &container_name).await
    } else if let Some(unit) = &svc.systemd_unit {
        systemd_log_stream(unit).await
    } else {
        return bad_request("not backed by a systemd unit or container");
    };
    match stream {
        Ok(s) => Sse::new(s).keep_alive(...).into_response(),
        Err(msg) => (StatusCode::BAD_GATEWAY, Json(json!({"error": msg}))).into_response(),
    }
}
```

Both branches produce a `Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>`
(`BoxedLogStream`) so the handler has one concrete return type regardless
of which backend served it — the same type-erasure axum already needs
whenever a handler can legitimately return more than one response shape.

**systemd** (new `control::systemd::tail_unit_logs`, alongside the
existing start/stop/restart in that module):
```rust
let mut child = tokio::process::Command::new("journalctl")
    .args(["-u", unit_name, "-n", "50", "-f", "--no-pager", "-o", "short-iso"])
    .stdout(Stdio::piped())
    .kill_on_drop(true)   // an abandoned SSE connection kills the process, not leaks it
    .spawn()?;
let stdout = child.stdout.take().unwrap();
let lines = LinesStream::new(BufReader::new(stdout).lines());
```
The `Child` itself is moved into the mapped stream (via a struct that owns
both) so it stays alive exactly as long as something is polling the
stream — when the SSE response future drops (client disconnects), the
`Child` drops, `kill_on_drop` fires, `journalctl -f` exits. No manual
process-tracking table needed.

**Docker/Podman** (new `control::docker::tail_container_logs`): connects
via `Docker::connect_with_socket` (same helper already used for control
actions), calls `.logs(name, Some(LogsOptions::<String> { follow: true,
stdout: true, stderr: true, tail: "50".into(), ..Default::default() }))`,
maps each `LogOutput::{StdOut,StdErr,...}` to its `message: Bytes` →
`String::from_utf8_lossy`. Bollard's stream is dropped (and the
underlying HTTP/socket connection with it) the same way the systemd
`Child` is — when the SSE future drops.

Both map their lines to `Event::default().data(line)` and swallow
mid-stream errors by ending the stream (matching how the existing
metrics/probe SSE endpoints already `filter_map` errors away) — a
disconnect just closes the connection; the frontend doesn't need special
error-frame handling for a first cut of this feature.

`tokio-stream`'s `wrappers::LinesStream` needs the `io-util` feature,
currently not enabled (only `sync` is, for `BroadcastStream`) —
`Cargo.toml` gains that feature flag. No new crate.

### 4b. Frontend

`HistoryModal` gains (admin-only, alongside the existing Controls row):
- A "Logs" toggle button.
- `log_lines: RwSignal<Vec<String>>`, capped to the last 500 entries
  client-side (a live tail is unbounded over time; the modal isn't a log
  archive).
- A scrolling `<pre>`-style panel, auto-scrolled to the newest line.

Wiring follows `pages/dashboard/mod.rs`'s existing probe-stream
`EventSource` precedent (the one place in this codebase already gated
`#[cfg(target_arch = "wasm32")]` for raw `web_sys`/DOM calls — `gloo_net`
and `spawn_local` elsewhere don't need that gate, confirmed in Feature 1),
with one difference: that listener is opened once for the app's lifetime
and intentionally leaked (`.forget()`). This one is opened and closed
repeatedly as an admin toggles Logs on a modal that itself opens and
closes, so the `EventSource` handle is held in a plain `Rc<RefCell<Option<EventSource>>>`
(not a reactive signal — nothing needs to react to the handle itself) and
explicitly `.close()`d when Logs is toggled off, the target service
changes, or the modal closes — so an abandoned tab doesn't hold either a
browser connection or a server-side `journalctl -f`/docker log stream open
indefinitely.

## 5. Dependencies

`tokio-stream`'s `io-util` feature — no new crate. Bollard's `logs()` was
checked via Context7 the same way `control/docker.rs` was in Feature 2;
the docs resolved to the newer `query_parameters` builder API (bollard
0.20) again, while this workspace pins 0.17, so the implementation uses
the older `LogsOptions::<String> { ..Default::default() }` struct-literal
shape already proven in this codebase (`discovery/docker.rs`'s
`ListContainersOptions::<String>`), with `cargo check` as the actual
arbiter of the pinned version's exact field names — same approach that
worked cleanly in Feature 2.

## 6. Files touched

Backend: `control/systemd.rs`, `control/docker.rs`, `api/services.rs`,
`api/openapi.rs` (SSE stream endpoints aren't schema-bearing, so this is
just the path registration), `Cargo.toml` (tokio-stream feature).
Frontend: `components/history_modal.rs`, `style/main.css`.

## 7. Risks & mitigations

| Risk | Mitigation |
|---|---|
| `journalctl` not installed (e.g. a minimal container image) | Subprocess spawn failure is caught and returned as a clean 502 with the real error, not a silent broken stream |
| Abandoned connection leaks a `journalctl -f` process or docker log stream | `kill_on_drop(true)` for the subprocess; both stream types are dropped (and their resources released) when the SSE future is dropped on client disconnect |
| Log output contains sensitive text | Admin-only, no read tier — stricter than Feature 1's history view |
| Unbounded memory growth in a long-open modal | Client caps retained lines to the most recent 500 |

## 8. Approved validation commands

Same as established: `cargo fmt --all -- --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test -p vexboard-server`,
`cargo build --release --bin vexboard-server`, `scripts/preflight.ps1`.
