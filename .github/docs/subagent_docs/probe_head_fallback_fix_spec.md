# Phase 1 Spec — probe_head_fallback_fix

**Date:** 2026-07-03

## Current State Analysis

`crates/vexboard-server/src/probe/uptime.rs:44-130` (`probe_service`) is the HTTP prober
used for every service dispatched to the URL branch (see `probe_head_fallback_fix`'s
predecessor, `remote_service_status_fix`, 2026-07-02, which fixed a separate dispatch bug
for Docker/Podman-discovered services and is unaffected by this change).

Current logic (lines 67-90):

```rust
let (status, latency_ms) = match client.head(&url).send().await {
    Ok(resp) => {
        let latency = start.elapsed().as_millis() as i64;
        if resp.status().is_success() || resp.status().is_redirection() {
            ("up".to_string(), Some(latency))
        } else {
            ("down".to_string(), Some(latency))
        }
    }
    Err(_) => {
        // HEAD failed — fall back to GET.
        let start2 = Instant::now();
        match client.get(&url).send().await {
            Ok(resp) => { /* same success check */ }
            Err(_) => ("down".to_string(), None),
        }
    }
};
```

## Problem Definition

1. **HEAD-only fallback bug:** GET fallback only triggers when the HEAD *request itself*
   errors (DNS failure, connection refused, TLS handshake failure, timeout). If the server
   responds to HEAD with a non-2xx/3xx HTTP status — very common, since many reverse
   proxies, SPA routers, and app frameworks return 404/405/500 for HEAD while serving GET
   correctly — the service is marked `"down"` immediately and GET is never attempted.
   Uptime Kuma avoids this class of bug entirely by defaulting to GET.

2. **Silent error swallowing:** both `Err(_)` arms discard the actual `reqwest::Error`
   with no logging. There is currently no way to determine from server logs *why* a given
   probe failed (timeout vs. DNS vs. TLS vs. connection refused vs. non-success status).
   This blocks diagnosing further reports of this kind, including whether TLS certificate
   validation (`danger_accept_invalid_certs(false)`, line 60 — left unchanged, out of
   scope per user decision) is a contributing factor for any specific service.

Together these cause services that are genuinely reachable to be recorded as `"down"`
on every probe tick — matching the reported symptom of remote/URL-probed services
"always showing Down even when the service is up and running."

## Proposed Solution

Restructure the HEAD/GET flow in `probe_service` so that:
- GET is attempted whenever the HEAD attempt does **not** yield a 2xx/3xx status —
  whether that's because the request errored, or because it returned a non-success
  status code.
- Both the HEAD-fallback trigger and the final GET failure are logged via `tracing`,
  matching the existing logging style used in `probe_systemd_unit`
  (`tracing::warn!(unit = %unit_name, "D-Bus unit state query failed: {e}")`).

```rust
let start = Instant::now();

let head_outcome = client.head(&url).send().await;
let (status, latency_ms) = match head_outcome {
    Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {
        ("up".to_string(), Some(start.elapsed().as_millis() as i64))
    }
    other => {
        match &other {
            Ok(resp) => tracing::debug!(
                url = %url, status = %resp.status(),
                "HEAD probe returned non-success status, falling back to GET"
            ),
            Err(e) => tracing::debug!(
                url = %url, error = %e,
                "HEAD probe request failed, falling back to GET"
            ),
        }
        let start2 = Instant::now();
        match client.get(&url).send().await {
            Ok(resp) => {
                let latency = start2.elapsed().as_millis() as i64;
                if resp.status().is_success() || resp.status().is_redirection() {
                    ("up".to_string(), Some(latency))
                } else {
                    tracing::warn!(
                        url = %url, status = %resp.status(),
                        "GET probe returned non-success status, marking service down"
                    );
                    ("down".to_string(), Some(latency))
                }
            }
            Err(e) => {
                tracing::warn!(url = %url, error = %e, "GET probe failed, marking service down");
                ("down".to_string(), None)
            }
        }
    }
};
```

This is a self-contained restructuring of the existing match expression — no new
functions, no signature changes, no new fields.

### Explicitly out of scope (per user decision)

TLS certificate leniency (`danger_accept_invalid_certs`) is **not** changed in this fix.
The user was unsure whether their affected remote services use self-signed/internal
certs. The new `tracing::warn!` logging added here will surface the actual `reqwest`
error (including TLS errors) on the next failure, giving a factual basis to decide
whether a TLS-leniency toggle is warranted as a separate, explicitly-scoped follow-up —
rather than silently weakening certificate validation now.

## Affected Files

1. `crates/vexboard-server/src/probe/uptime.rs` — `probe_service` function only
   (lines ~64-91). No other function in this file is touched.

## Implementation Steps

1. Replace the HEAD/GET match block in `probe_service` (uptime.rs) with the restructured
   version above.
2. No changes to `probe_systemd_unit`, dispatch sites (`probe/mod.rs`,
   `api/services.rs`), DB schema, or frontend — none of those are implicated by this bug.

## Dependencies

No new dependencies. `tracing` is already a workspace dependency and already used
elsewhere in this file. No Context7 lookup required.

## Configuration Changes

None.

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p vexboard-server`
- `cargo build --release --bin vexboard-server`

All approved per CLAUDE.md. No FORBIDDEN COMMANDS used.

## Risks and Mitigations

- **Risk:** A server that returns a non-success status to HEAD *and* GET now takes two
  round trips before recording "down" instead of one for HEAD-only failures that were
  already erroring at the transport level (no behavior change there — GET fallback on
  transport error already existed). For the newly-covered case (non-success HEAD status),
  this trades one extra request for correctness. At the default 30s probe interval this
  is negligible.
- **Risk:** Increased log volume from `tracing::warn!` on every failed probe tick for a
  genuinely-down service. Mitigation: this matches the existing precedent in
  `probe_systemd_unit`, and `tracing::warn!` is appropriate for the down-service case
  (users actively want durable down-status log evidence, per the original bug report).
- **Risk:** Does not fix a TLS-certificate-related cause if that turns out to be the
  actual root cause for the user's specific services. Mitigation: intentional and
  explicit per user's "not sure" answer — the new logging is the mechanism to confirm or
  rule this out before scoping a follow-up change.
