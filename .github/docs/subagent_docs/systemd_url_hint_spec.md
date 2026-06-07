---
# Phase 1 Specification: Systemd Service URL Hint Detection
Feature: `systemd_url_hint`
Created: 2026-06-07
---

## Current State Analysis

### What works today
- **Docker/Podman** containers: `url_hint` is populated in `docker.rs` by inspecting the container's `ports` field (exposed/mapped port bindings). The hint is constructed as `http://{host}:{port}` and flows through the API to the frontend.
- **Frontend**: `discovery_panel.rs` already reads `url_hint` and pre-populates the URL field in `EditModal` via `EditFormData.url = unit.url_hint.clone().unwrap_or_default()`.

### What does NOT work
- **Systemd services**: `url_hint` is hardcoded to `None` in `systemd.rs` line 115. Every systemd service discovered shows an empty URL field in the "Add Discovered Service" modal regardless of what port the service listens on.

### Data flow (already correct end-to-end)
```
discovery/systemd.rs  →  DiscoveredUnit { url_hint: Option<String> }
  ↓ stored in Arc<RwLock<Vec<DiscoveredUnit>>>
api/mod.rs (GET /api/v1/discovery)  →  JSON serialized
  ↓ fetched by frontend
discovery_panel.rs  →  DiscoveredUnitFe { url_hint: Option<String> }
  →  EditFormData { url: url_hint.unwrap_or_default() }
  →  EditModal URL input field (pre-filled)
```

The only gap is the backend not setting `url_hint` for systemd services.

---

## Problem Definition

When a user discovers a systemd service (e.g., `nginx.service`, `vexboard-server.service`, any NixOS-managed service) and clicks "Add" to add it to the dashboard, the URL field is empty. The user has to manually find and type the URL. VexBoard can detect the port from Linux system APIs without any user intervention.

---

## Proposed Solution Architecture

### Detection strategy (two-stage with fallback)

**Stage 1 — Socket activation (D-Bus)**

Many services on NixOS and modern systemd use socket activation. For these:
1. Use the unit's existing `object_path` (already in `UnitInfo`) to create a D-Bus proxy for the `org.freedesktop.systemd1.Service` interface
2. Read the `Sockets` property → array of object paths for associated `.socket` units
3. For each socket unit, read its `Listen` property → array of `(type, address)` pairs
4. Parse TCP port from addresses (skip Unix domain sockets)
5. Construct `http://localhost:{port}`

**Stage 2 — Direct process listener (procfs)**

For services that do NOT use socket activation (e.g., nginx, many NixOS services):
1. Read `MainPID` property from `org.freedesktop.systemd1.Service`
2. Open `/proc/{pid}/net/tcp` and `/proc/{pid}/net/tcp6`
3. Parse lines where state column == `0A` (TCP_LISTEN)
4. Decode the hex port from the `local_address` field
5. Construct `http://localhost:{port}`

**Host selection**: Use `localhost` — consistent with Docker's unix-socket behavior (`socket_host()` returns `"localhost"` for unix socket paths). The user's browser connects to VexBoard, and VexBoard runs on the same host as the services. The user can edit the hostname in the URL field if needed.

### Why `localhost` not the server's external IP?

- Docker's `socket_host()` already uses `localhost` for unix socket connections (the common case)
- The server's bound IP is `0.0.0.0` (listens on all), not a useful external IP
- Getting the "real" external IP requires heuristics (first non-loopback interface, etc.) that are fragile and wrong on multi-homed hosts
- Users adding services to the dashboard typically access both VexBoard and the service via the same hostname/IP — they can trivially change `localhost` to a hostname if needed
- Providing a port is the primary value; the host is secondary

---

## Implementation Steps

### Files to modify

**Only one file changes:** `crates/vexboard-server/src/discovery/systemd.rs`

No new dependencies, no frontend changes, no config changes, no DB migrations.

### Changes to `systemd.rs`

1. **Add two new D-Bus proxy trait definitions** using `#[zbus::proxy]`:

   ```rust
   // Service-level properties
   #[zbus::proxy(
       interface = "org.freedesktop.systemd1.Service",
       default_service = "org.freedesktop.systemd1",
       default_path = "/placeholder",
   )]
   trait ServiceUnit {
       #[zbus(property)]
       fn sockets(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

       #[zbus(property)]
       fn main_pid(&self) -> zbus::Result<u32>;
   }

   // Socket-level properties
   #[zbus::proxy(
       interface = "org.freedesktop.systemd1.Socket",
       default_service = "org.freedesktop.systemd1",
       default_path = "/placeholder",
   )]
   trait SocketUnit {
       #[zbus(property)]
       fn listen(&self) -> zbus::Result<Vec<(String, String)>>;
   }
   ```

2. **Add `detect_url_hint` async function**:

   ```rust
   async fn detect_url_hint(
       connection: &zbus::Connection,
       object_path: &zvariant::OwnedObjectPath,
   ) -> Option<String> {
       // Stage 1: socket activation
       if let Some(port) = detect_port_via_sockets(connection, object_path).await {
           return Some(format!("http://localhost:{port}"));
       }
       // Stage 2: procfs via MainPID
       if let Some(port) = detect_port_via_proc(connection, object_path).await {
           return Some(format!("http://localhost:{port}"));
       }
       None
   }
   ```

3. **Add `detect_port_via_sockets`**: queries `Sockets` then `Listen` per socket, parses TCP port from addresses.

4. **Add `detect_port_via_proc`**: queries `MainPID`, reads `/proc/{pid}/net/tcp[6]`, parses LISTEN rows.

5. **Add `parse_port_from_listen_address`** helper: pure fn, handles:
   - `"0.0.0.0:8080"`, `"[::]:8080"`, `"127.0.0.1:8080"` → split on last `:`, parse port
   - `"8080"` → parse whole string as port
   - `"/run/app.sock"`, `""` → return None (Unix socket or empty)

6. **Add `parse_tcp_listen_port`** helper: reads one proc/net/tcp file, returns first LISTEN port.

7. **Call `detect_url_hint` in `discover_units`**: replace the hardcoded `url_hint: None` with the detected value.

### Port parsing — procfs format

`/proc/{pid}/net/tcp` line format (after header):
```
 sl  local_address rem_address   st ...
 0: 00000000:1F90 00000000:0000 0A ...
```
- Column 1: `local_address` = `{hex_ip}:{hex_port}` (little-endian IP, big-endian port)
- Column 3: `st` = `0A` for TCP_LISTEN
- Port is in column 1 after the `:`, hex, big-endian: `0x1F90 = 8080`

For `/proc/{pid}/net/tcp6`:
```
 0: 00000000000000000000000000000000:1F90 ... 0A ...
```
Same port format; only the IP portion differs.

### Port selection heuristic

When multiple LISTEN ports are found:
- Return the first port found (lowest file line number in tcp, then tcp6)
- This is deterministic and handles 99% of single-web-port services

---

## Dependencies

No new Cargo dependencies. All required tools are already available:
- `zbus = "5"` — already in workspace
- `std::fs` — standard library
- `zvariant::OwnedObjectPath` — already imported via `use zbus::zvariant`

---

## Build/Test Commands (Phase 3)

**Approved safe commands only (per CLAUDE.md):**

1. `cargo fmt --all -- --check` — formatting check, zero compilation cost
2. `cargo clippy --workspace -- -D warnings` — lint, compiles server crate only  
3. `cargo test --workspace` — unit tests (backend only; frontend has no native targets)
4. `cargo build --release --bin vexboard-server` — full backend build verification

**NOT used (FORBIDDEN):**
- `cargo build` (bare) — workspace build includes WASM-only frontend
- `cargo build --workspace` — same reason
- `trunk build` — Trunk not confirmed installed; frontend is unchanged anyway

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| D-Bus permission denied reading unit properties | Wrap in `if let Ok/Err` — detection failure → `url_hint = None`, discovery still works |
| Service has no MainPID (type=oneshot already ran) | `main_pid == 0` check → skip procfs read |
| `/proc/{pid}/net/tcp` not readable (permissions) | `std::fs::read_to_string` returns `Err` → try tcp6 → fall through to None |
| Service listens only on Unix socket, not TCP | Both stages find no TCP port → `url_hint = None` (correct, no URL to suggest) |
| Multiple listening ports (e.g., HTTP + HTTPS) | Return first found (user can edit) |
| Port 80/443 behind reverse proxy | No special handling needed; url_hint is editable |
| Performance: extra D-Bus calls per discovery pass | Typically < 20 qualifying services; D-Bus IPC is <1ms/call; discovery runs every 60s |
| zbus proxy default_path placeholder | Must use `.builder(&connection).path(object_path)?.build().await?` pattern |

---

## Spec File Path

`.github/docs/subagent_docs/systemd_url_hint_spec.md`
