# Spec: Bug Fix — hardcoded localhost in Docker URL hints

## Issue (LOW — audit 1.8 / 1.2)

`discovery/docker.rs:135`:
  `.map(|port| format!("http://localhost:{port}"))`

This always uses `localhost` as the host in the generated URL hint, which
is incorrect for:
- TCP Docker hosts (e.g. `tcp://192.168.1.10:2375`) — containers are on
  a remote machine; the URL should use that host's address
- Containers bound to a specific IP (e.g. port mapping `192.168.1.5:8080→80`)
  — the bound IP is more accurate than `localhost`

## Proposed Fix

### `socket_host(socket: &str) -> &str`
A small pure helper that returns the right host string for a given socket:
- Starts with `/` → Unix socket on local machine → `"localhost"`
- Starts with `tcp://host:port` or `http://host:port` → extract the host segment
- Anything else → `"localhost"` (safe fallback)

### URL hint construction
Replace the simple `.map(|port| format!(...))` chain with a closure that:
1. Extracts `bound_ip` from `p.ip` (bollard `Port.ip: Option<String>`)
2. If `bound_ip` is empty, `"0.0.0.0"`, or `"::"` (wildcard) → use `socket_host`
3. Otherwise use the specific bound IP directly
4. Formats `"http://{host}:{port}"`

Both `socket_host` and the URL hint logic live in `discover_from_socket`,
which already receives `socket: &str`.

## Files Modified
- `crates/vexboard-server/src/discovery/docker.rs`

## Build/Test Commands
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `scripts/preflight.sh`
