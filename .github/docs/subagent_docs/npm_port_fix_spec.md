# Spec: Fix nginx proxy manager wrong port (8444 instead of 81)

## Current State Analysis

### Bug
Nginx Proxy Manager is discovered with port 8444 instead of 81 (the admin web UI port).

### Discovery Paths

There are two independent discovery paths that compute `url_hint`:

**Path A — `docker.rs` (Docker API, lines 193–207)**
Used when nginx proxy manager runs as a direct (non-systemd) Docker/Podman container.
Uses `.find(|p| p.public_port.is_some())` — picks the **first** port in the Docker API
response that has a host-side binding. Docker does not guarantee ordering, so if 8444
appears first, it is returned as the URL hint with no filtering.

**Path B — `systemd.rs` `parse_docker_port_output` (lines 313–358)**
Used when nginx proxy manager runs as a systemd-managed OCI container (Stage 2 OCI
detection). Runs `podman port` / `docker port` and parses the output with tiered
selection. The HTTPS skip list is `matches!(container_port, 443 | 8443 | 4443)`.

**Missing: `8444` in the HTTPS skip list.**
When nginx proxy manager is configured with `8444/tcp → 8444` (the admin HTTPS port
mapped directly, as opposed to `443/tcp → 8444`), `container_port` is 8444, which
is NOT in the skip list. Since 8444 ≠ 80 it is treated as a "preferred" (tier 1) port,
beating port 81 in tier 1 ordering (first-seen wins due to `get_or_insert`). If the
runtime outputs the 8444 line before the 81 line, 8444 is returned — WRONG.

The existing test at line 751 only covers the `443/tcp → 8444` case (correct), not
the `8444/tcp → 8444` case (broken).

## Problem Definition

Two distinct bugs:
1. **`systemd.rs`**: `8444` missing from HTTPS skip list in `parse_docker_port_output`
2. **`docker.rs`**: Naive `.find()` picks first port; no SSL/HTTPS filtering at all

## Proposed Solution

### Fix 1 — `systemd.rs`
Add `8444` to the HTTPS container-port skip list:
```rust
if matches!(container_port, 443 | 8443 | 4443 | 8444) {
```
Add a unit test covering the `8444/tcp → 8444` mapping alongside `81/tcp → 81`.

### Fix 2 — `docker.rs`
Replace the naive `.find()` with tiered selection using `p.private_port` (container port)
from the bollard `Port` struct:
- **Tier 1 (preferred)**: `private_port` ≠ 80 AND ≠ 443/8443/4443/8444
- **Tier 2 (fallback_80)**: `private_port` == 80
- **Tier 3 (any)**: first port with a public binding

Selection: `preferred.or(fallback_80).or(any_port)`

## Implementation Steps

1. Edit `crates/vexboard-server/src/discovery/systemd.rs` line 347:
   - Add `8444` to the `matches!` skip list
   - Add one new test: `test_parse_docker_port_output_npm_8444_direct`

2. Edit `crates/vexboard-server/src/discovery/docker.rs` lines 193–207:
   - Replace `.find()` with three-tier port selection loop

## Dependencies
No new dependencies. Internal changes only. Context7 not required.

## Approved Validation Commands
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`

## Risks and Mitigations
- **Risk**: 8444 is not exclusively an SSL port — some services might use it for HTTP.
  **Mitigation**: The tier-3 fallback still returns 8444 when it's the only exposed port,
  so those services still get a URL hint. Only when a non-SSL, non-80 alternative exists
  (like port 81) will 8444 be deprioritized.
- **Risk**: Docker API port ordering varies by runtime.
  **Mitigation**: Tiered selection is order-independent; all ports are scanned before picking.
