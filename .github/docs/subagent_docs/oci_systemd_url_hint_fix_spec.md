---
# Phase 1 Specification: OCI-Aware Systemd URL Hint Detection
Feature: `oci_systemd_url_hint_fix`
Created: 2026-06-07
---

## Current State Analysis

The current `detect_url_hint` in `discovery/systemd.rs` has three detection stages:

1. **Stage 1 — Socket activation**: reads the unit's `Sockets` D-Bus property, then each socket's `Listen` property.
2. **Stage 2 — MainPID + inode matching**: reads `MainPID` via D-Bus, collects socket inodes from `/proc/{pid}/fd/`, matches against `/proc/{pid}/net/tcp[6]`.
3. **Stage 3 — cgroup.procs fallback**: scans all PIDs in the service cgroup via `/sys/fs/cgroup/system.slice/{unit}/cgroup.procs` and applies the same inode-matched scan.

`listen_port_for_pid` has an inode-matching path and a UID-matching fallback: when `/proc/{pid}/fd/` is unreadable, it reads the effective UID from `/proc/{pid}/status` and matches against the `uid` column of the TCP table.

---

## Problem Definition

**Bug**: OCI containers running as systemd services (e.g., nginx proxy manager via Podman) get the wrong URL hint — port 631 (CUPS) instead of the correct container port (8444).

**Root cause**: When a container runtime (podman, docker) is the `MainPID` of a systemd service:

1. The container's listening sockets are bound by helper processes (`rootlessport`, `pasta`, `slirp4netns`, `docker-proxy`) inside the container's network namespace — not by the podman/docker process itself.
2. `collect_socket_inodes(podman_pid)` returns either `Some(empty)` (podman has no open sockets → early return `None`) or `Some(non-matching)` (socket inodes don't match port 8444 in the host TCP table).
3. Stage 3 (cgroup scan) scans helper processes inside the cgroup. When one of them has an unreadable `/proc/{pid}/fd/` dir, the code falls back to **UID matching**.
4. Both CUPS and the container service run as root (uid 0). CUPS port 631 appears earlier in `/proc/net/tcp` than the container's port 8444.
5. UID matching incorrectly returns port 631.

**Why not just exclude port 631**: CUPS can use any port; this is incidental. The real fix is to detect OCI services and query the container runtime directly.

---

## Proposed Solution Architecture

### New Stage 2 — OCI Detection (inserted before existing procfs stages)

After Stage 1 (socket activation) fails:

1. Read `MainPID` from D-Bus (same as current stage 2 does).
2. Read `/proc/{MainPID}/exe` symlink to get the binary name.
3. If the binary is `podman`, `podman-remote`, or `docker` → this is an OCI service.
4. Query the container runtime for port bindings:
   - Derive container name candidates from unit name (see below)
   - Run `podman port <name>` or `docker port <name>` for each candidate
   - Parse output, return first valid port
5. If OCI detected but port query fails (container stopped, name mismatch, etc.) → return `None` without falling through to UID matching. This prevents the CUPS false positive.
6. If not OCI → proceed to existing stages 3/4 (procfs inode matching, cgroup scan) unchanged.

### Container name derivation

For `<unit-name>.service`:
- Candidate 1: `<unit-name>` (e.g., `nginx-proxy-manager.service` → `nginx-proxy-manager`)
- Candidate 2: `systemd-<unit-name>` (quadlet containers are named `systemd-<name>`)

Try each candidate in order; stop at the first successful `podman port` / `docker port` call.

### `podman port` / `docker port` output parsing

```
80/tcp -> 0.0.0.0:80
81/tcp -> 0.0.0.0:81
443/tcp -> 0.0.0.0:443
8444/tcp -> 0.0.0.0:8444
```

Parse: split on `->`, take the RHS, rsplit on `:`, parse as `u16`. Return the first valid port (≥ 1).

### Control flow in `detect_url_hint`

```
Stage 1: socket activation
  → found port? return it

Stage 2: OCI detection via /proc/{MainPID}/exe
  → Is OCI + port found?  return it
  → Is OCI + no port?     return None  ← prevents CUPS false positive
  → Not OCI?              continue ↓

Stage 3: MainPID + inode-matched procfs (unchanged, non-OCI only)
  → found port? return it

Stage 4: cgroup.procs scan (unchanged, non-OCI only)
  → found port? return it

→ None
```

### New types

```rust
enum OciDetect {
    Found(u16),   // OCI service, port discovered
    NoPort,       // OCI service, port not discoverable (skip UID matching)
    NotOci,       // Not a container service, proceed normally
}
```

---

## Implementation Steps

**File modified:** `crates/vexboard-server/src/discovery/systemd.rs`

1. Add `OciDetect` enum (private, within the module).
2. Add `async fn oci_detect(connection, path, unit) -> OciDetect`:
   - Calls `get_main_pid(connection, path, unit) -> Option<u32>` (factored from current `detect_via_main_pid`)
   - Reads `/proc/{pid}/exe` symlink
   - If basename is `podman`, `podman-remote`, or `docker` → calls `query_container_ports(runtime, unit)`
   - Returns `OciDetect::Found(port)`, `OciDetect::NoPort`, or `OciDetect::NotOci`
3. Add `async fn query_container_ports(runtime_bin: &str, unit: &str) -> Option<u16>`:
   - Derives candidate names from unit name
   - Runs `podman port <name>` / `docker port <name>` via `tokio::process::Command`
   - Calls `parse_docker_port_output` on stdout
4. Add `fn parse_docker_port_output(output: &str) -> Option<u16>`:
   - Pure function, parses `"proto/tcp -> host:port"` lines
   - Returns first valid port
5. Modify `detect_url_hint` to call `oci_detect` as Stage 2, with the three-way branch described above.
6. Refactor `detect_via_main_pid` to accept a pre-fetched `pid: u32` (to avoid re-reading MainPID twice) or leave as-is and accept the small overhead of a second D-Bus read.

**No changes to:** frontend, API, DB schema, config, or any other file.

---

## Dependencies

No new Cargo dependencies.

- `tokio::process::Command` — already in scope via `tokio` in workspace.
- `zbus` — already in workspace.

---

## Build/Test Commands (Phase 3)

Per CLAUDE.md approved safe commands:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace`
4. `cargo build --release --bin vexboard-server`

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| `podman`/`docker` not in PATH for server process | `Command::new("podman")` returns `Err` → `None` → falls through to `OciNoPort` → returns `None` safely |
| Container stopped when discovery runs | `podman port <name>` exits non-zero → `None` → `OciNoPort` |
| Container name doesn't match unit name | Both `<name>` and `systemd-<name>` are tried; if both fail → `OciNoPort` |
| `/proc/{pid}/exe` unreadable (different user) | `tokio::fs::read_link` returns `Err` → `None` → `NotOci` → existing stages continue |
| Multiple ports exposed (e.g., 80, 81, 443, 8444) | First port returned; user can override in edit modal |
| Non-zero MainPID but process already exited | `read_link` returns `Err` (no `/proc/{pid}`) → `NotOci` |
| OCI service bound to a non-HTTP port | URL hint `http://localhost:N` may be inaccurate, but is better than CUPS port 631; user edits it |
