---
# Phase 1 Specification: Suppress Container-Socket Entries for Systemd-Managed OCI Services
Feature: `oci_dedup_docker_discovery`
Created: 2026-06-07
---

## Current State Analysis

VexBoard runs two independent discovery loops:
- **systemd discovery** (`discovery/systemd.rs`): finds active `.service` units via D-Bus; tags entries `source: "systemd"`.
- **container discovery** (`discovery/docker.rs`): queries Docker/Podman sockets; tags entries `source: "docker"` or `source: "podman"`.

Both write into the same shared `DiscoveryList`. Each loop replaces only its own source entries on every pass. The two loops have no cross-reference and can produce duplicate entries for the same real service.

---

## Problem Definition

When an OCI container (e.g. Nginx Proxy Manager) is run as a systemd service via Podman quadlets or `podman generate systemd`:

1. **systemd discovery** correctly finds `nginx-proxy-manager.service`.
2. **container discovery** also finds the underlying container, which may be named `docker-nginx-proxy-manager` (the container name set at creation time).

The discovery panel shows both entries. The container entry is confusing because:
- Its name (`docker-nginx-proxy-manager`) implies it is a Docker-managed container.
- It is actually managed by a systemd unit and should appear only via systemd discovery.

---

## Proposed Solution

### Two-tier deduplication in `discover_from_socket`

**Tier 1 — `PODMAN_SYSTEMD_UNIT` label (authoritative)**

Podman automatically sets the `PODMAN_SYSTEMD_UNIT` label on every container managed via a systemd unit (quadlets since Podman 4.4, `podman generate systemd` since earlier). The label value is the fully-qualified unit name (e.g. `nginx-proxy-manager.service`).

When this label is present, skip the container unconditionally. The systemd discovery loop will cover it.

**Tier 2 — cross-reference against current systemd discoveries (belt-and-suspenders)**

Pass the set of systemd unit names currently in the `DiscoveryList` into `discover_from_socket`. For each container, derive a candidate unit name: `<container-name>.service`. If that name is already in the systemd set, skip the container.

This covers hand-crafted systemd wrappers that do not add the Podman label.

### Also check DB for systemd_unit

The existing DB claimed-check uses `display_name` and `systemd_unit` — but with the container name `docker-nginx-proxy-manager` as the bind value. Extend the check: if the container has a `PODMAN_SYSTEMD_UNIT` label, also query whether that unit name is already claimed in the DB.

---

## Implementation Steps

**File modified:** `crates/vexboard-server/src/discovery/docker.rs`

1. Add `use std::collections::HashSet;`.
2. Modify `discover_containers` signature: read the `DiscoveryList` before the socket loop to collect the set of systemd unit names currently in the list.
3. Pass `systemd_units: &HashSet<String>` to `discover_from_socket`.
4. In `discover_from_socket`, for each container:
   a. Read `PODMAN_SYSTEMD_UNIT` label → if present, skip (tier 1).
   b. Derive `candidate_unit = format!("{name}.service")` → if in `systemd_units`, skip (tier 2).
   c. Extend existing DB claimed-check: if `PODMAN_SYSTEMD_UNIT` label is present, also check `WHERE systemd_unit = <label-value>`.

---

## Dependencies

No new Cargo dependencies.

---

## Build/Test Commands (Phase 3)

Per CLAUDE.md approved safe commands:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test -p vexboard-server`
4. `cargo check --bin vexboard-server`

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| systemd discovery hasn't run yet when docker discovery first runs | Tier 2 set would be empty; container appears once, disappears on next pass when systemd has run. Tier 1 (label) is always available. |
| Container doesn't have `PODMAN_SYSTEMD_UNIT` and name doesn't match unit name | Container still appears; this is correct — it genuinely isn't managed by a known systemd unit. |
| Docker containers with compose labels | Not affected — Docker compose containers don't have `PODMAN_SYSTEMD_UNIT`; their names typically don't end in `.service`. |
