# Spec: OCI Systemd Service — Name Display and Port Detection Fixes

## Current State Analysis

### Issue 1 — Display name includes runtime prefix

**File:** `crates/vexboard-frontend/src/components/discovery_panel.rs:15-17`

```rust
fn display_name(&self) -> String {
    self.unit_name.trim_end_matches(".service").to_string()
}
```

Systemd units that manage OCI containers via Docker or Podman are named by
convention as `docker-<container-name>.service` (Docker) or
`podman-<container-name>.service` (Podman). The current logic strips only the
`.service` suffix, leaving the runtime prefix in the displayed name.

**Result:** `docker-nginx-proxy-manager.service` → displayed as
`docker-nginx-proxy-manager`. This is confusing, sorts under "D" instead of
"N", and leaks an implementation detail into the UI.

**Expected:** `docker-nginx-proxy-manager.service` → `Nginx Proxy Manager`

---

### Issue 2 — Port detection returns first mapped port regardless of protocol

**File:** `crates/vexboard-server/src/discovery/systemd.rs:308-323`

```rust
fn parse_docker_port_output(output: &str) -> Option<u16> {
    for line in output.lines() {
        let Some(arrow) = line.find("->") else { continue; };
        let host_part = line[arrow + 2..].trim();
        if let Some(port_str) = host_part.rsplit(':').next() {
            if let Ok(port) = port_str.trim().parse::<u16>() {
                if port > 0 {
                    return Some(port);
                }
            }
        }
    }
    None
}
```

This returns the **first** host port found. For a container like Nginx Proxy
Manager which exposes multiple ports, the output of `docker port` is
ordered by container port number, not significance:

```
443/tcp -> 0.0.0.0:8444    ← container HTTPS port maps to host 8444
80/tcp  -> 0.0.0.0:80      ← HTTP proxy traffic
81/tcp  -> 0.0.0.0:81      ← Admin web UI (correct target)
```

The function currently returns `8444` (the host port for the HTTPS proxy),
which is an HTTPS port and not the admin web UI.

**Expected:** `81` (the admin HTTP interface)

---

## Problem Definition

1. **Naming:** OCI containers managed through systemd units carry a runtime
   prefix in their unit name that is meaningless to end users and degrades
   sort order. The name in the discovery panel should reflect the actual
   service, not the container runtime used to run it.

2. **Port selection:** When a container exposes multiple ports, the current
   code blindly returns the first one in the `docker port` output. HTTPS ports
   (container port 443 and common HTTPS alternates) and generic HTTP proxy
   ports (container port 80) should be deprioritised in favour of likely
   admin/web-UI ports.

---

## Proposed Solution Architecture

### Fix 1 — Frontend: smarter `display_name()` (discovery_panel.rs)

Strip runtime prefixes `docker-` and `podman-` from the base unit name before
title-casing. Convert hyphens and underscores to spaces and title-case each
word.

**Algorithm:**
1. Strip `.service` suffix.
2. If the result starts with `docker-` or `podman-`, strip that prefix.
3. Split on `-` or `_`.
4. Capitalise the first character of each segment.
5. Join with spaces.

**Examples:**
| Unit name | Result |
|-----------|--------|
| `docker-nginx-proxy-manager.service` | `Nginx Proxy Manager` |
| `podman-whoami.service` | `Whoami` |
| `nginx.service` | `Nginx` |
| `my_custom-service.service` | `My Custom Service` |

No new dependencies required. Pure string manipulation in stable Rust.

---

### Fix 2 — Backend: smarter port selection in `parse_docker_port_output()` (systemd.rs)

Parse **both** the container port and the host port from each line of
`docker port` / `podman port` output.

Apply a three-tier preference:

| Tier | Rule | Rationale |
|------|------|-----------|
| **Skip** | Container port is 443, 8443, or 4443 | HTTPS — VexBoard uses HTTP `http://` URLs so HTTPS ports are not useful as url_hints |
| **Low priority** | Container port is 80 | Generic HTTP proxy traffic; most web-facing services use 80 for pass-through traffic, not admin UIs |
| **Prefer** | Any other TCP port | Likely an admin/management interface |

Selection order:
1. First port in tier "Prefer"
2. First port in tier "Low priority" (80)
3. First port overall (raw fallback if all are HTTPS)

This fixes the NPM case (`443→8444` skipped, `80→80` low priority, `81→81`
returned) without breaking single-port containers that only expose port 80.

---

## Implementation Steps

### Step 1: `discovery_panel.rs` — update `display_name()`

Replace the one-liner with the multi-step algorithm described above.

### Step 2: `systemd.rs` — update `parse_docker_port_output()`

Rewrite to collect `(container_port, host_port)` pairs, then apply the
three-tier selection. Update the existing unit tests to cover:
- multi-port output with HTTPS port first (NPM-like scenario)
- single port 80 fallback
- all-HTTPS fallback

---

## Dependencies

No new crates required. Both changes use only the Rust standard library and
existing project infrastructure.

---

## Build/Test Commands (Phase 3)

All commands are in the approved safe list and do not appear in FORBIDDEN COMMANDS:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`

Resource cost: low — no WASM target, no Docker build, no full workspace native build.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Heuristic still picks wrong port for some containers | url_hint is editable at claim time; user can override |
| Stripping `docker-` prefix collides with a real service named `docker-something` | Prefix stripping is UI-only (display name); `unit_name` and DB record are unchanged |
| Tests for `parse_docker_port_output` break | Update existing tests to match new expected behaviour and add new cases |
