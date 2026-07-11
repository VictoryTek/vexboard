# BUG-6 — Per-Probe `reqwest::Client` + Timeout-less Fallback — Spec

## Current State Analysis

`probe_service` (`crates/vexboard-server/src/probe/uptime.rs:46-62`) builds a brand-new
`reqwest::Client` on every single invocation:
```rust
let client = reqwest::Client::builder()
    .timeout(timeout)
    .danger_accept_invalid_certs(false)
    .build()
    .unwrap_or_default();
```
This is called once per service per probe tick (`crates/vexboard-server/src/probe/mod.rs:68`,
inside the per-service spawned task) and once more per newly-created service
(`crates/vexboard-server/src/api/services.rs:286-293`, the immediate post-create probe). Each
call constructs a fresh connection pool and TLS configuration instead of reusing one, discarding
whatever keep-alive connections were just established. Additionally, `.unwrap_or_default()`
silently substitutes `reqwest::Client::default()` — which has **no timeout at all** — if
`.build()` ever fails, defeating the entire purpose of the `timeout` parameter exactly when
something is already wrong.

The notification pipeline (`crates/vexboard-server/src/main.rs:228-231`) already demonstrates
the correct pattern: build one `reqwest::Client` once at startup and pass it by reference/clone
into the long-running task (`notify::notification_loop`, which stores it as a field —
`crates/vexboard-server/src/notify.rs:20`).

## Problem Definition

1. A new HTTP client (and its connection pool/TLS state) is constructed on every probe, wasting
   connection reuse and adding avoidable per-probe overhead.
2. A `Client::builder().build()` failure — however rare — silently produces a client with no
   timeout, meaning a hung/slow endpoint could block a probe task indefinitely instead of the
   configured timeout ever taking effect.

## Proposed Solution

Build one shared `reqwest::Client` once at server startup (mirroring the `notify_client`
pattern), with `config.probe.timeout_secs` baked in at construction time (this value never
changes at runtime — it's read from config once at startup, matching existing behavior since
`timeout` was always derived from the same config field on every call). Store it on `AppState`
(`probe_client`) so the `create_service` immediate-probe path can reuse it, and pass it as a
parameter into `start_probe_loop` for the scheduler's per-tick probes. `reqwest::Client` is
cheap to `.clone()` (internally `Arc`-wrapped connection pool), matching how `db: SqlitePool` is
already cloned per spawned task in the same functions.

Treat `Client::builder().build()` failure as a fatal startup error (`?` propagation in `main()`,
which already returns `anyhow::Result<()>` and already propagates other startup failures the
same way, e.g. `session_store.migrate().await?`) instead of silently degrading to a
timeout-less client.

## Implementation Steps

### 1. `crates/vexboard-server/src/main.rs`

Before building `AppState` (line ~187), add:
```rust
let probe_client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(config.probe.timeout_secs))
    .danger_accept_invalid_certs(false)
    .build()?;
```
Add `probe_client: probe_client.clone()` to the `AppState` struct literal.

At the probe-loop spawn site (line ~213-218), pass the client through:
```rust
let probe_config = config.probe.clone();
let probe_db = db.clone();
let probe_tx_clone = probe_tx.clone();
let probe_loop_client = probe_client.clone();
tokio::spawn(async move {
    probe::start_probe_loop(probe_db, probe_config, probe_tx_clone, probe_loop_client).await;
});
```

### 2. `crates/vexboard-server/src/main.rs` — `AppState`

Add field:
```rust
pub probe_client: reqwest::Client,
```

### 3. `crates/vexboard-server/src/probe/mod.rs`

Change `start_probe_loop`'s signature to accept the shared client:
```rust
pub async fn start_probe_loop(
    db: SqlitePool,
    config: ProbeConfig,
    status_tx: broadcast::Sender<uptime::ProbeEvent>,
    client: reqwest::Client,
) {
```
Remove the now-unused `let timeout = Duration::from_secs(config.timeout_secs);` line inside the
per-service spawn body (`config.timeout_secs` is no longer read here — the timeout is already
baked into `client`). Clone `client` alongside the existing `db`/`tx` clones before
`tokio::spawn`, and change the `probe_service` call to pass `&client` instead of `timeout`:
```rust
} else if svc.url.is_some() {
    uptime::probe_service(&db, &svc, &client, max_history, &tx).await;
}
```

### 4. `crates/vexboard-server/src/probe/uptime.rs`

Change `probe_service`'s signature from taking `timeout: Duration` to taking the shared client,
and remove the internal `reqwest::Client::builder()...` block entirely:
```rust
pub async fn probe_service(
    db: &SqlitePool,
    svc: &Service,
    client: &reqwest::Client,
    max_history: u64,
    tx: &broadcast::Sender<ProbeEvent>,
) {
    let url = match &svc.url {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return,
    };

    let start = Instant::now();
    // ...rest unchanged, using `client.head(&url)` / `client.get(&url)` as before...
```

### 5. `crates/vexboard-server/src/api/services.rs`

In `create_service`'s immediate-probe background task (lines ~256-296), replace:
```rust
let timeout_secs = state.config.probe.timeout_secs;
let max_history = state.config.probe.max_history;
```
with:
```rust
let probe_client = state.probe_client.clone();
let max_history = state.config.probe.max_history;
```
(drop `timeout_secs` entirely — no longer needed) and inside the spawned block, replace:
```rust
let timeout = Duration::from_secs(timeout_secs);
...
} else if svc.url.is_some() {
    probe::uptime::probe_service(
        &probe_db,
        &svc,
        timeout,
        max_history,
        &probe_tx,
    )
    .await;
}
```
with:
```rust
} else if svc.url.is_some() {
    probe::uptime::probe_service(&probe_db, &svc, &probe_client, max_history, &probe_tx).await;
}
```
(remove the now-unused `let timeout = Duration::from_secs(timeout_secs);` line and the
now-unused `std::time::Duration` import if it becomes otherwise unused in the file — verify via
`cargo clippy`).

## Dependencies

None new — `reqwest` is already a dependency; the change reuses the existing
`Client::builder()` API, just relocated and called once instead of per-probe.

## Configuration Changes

None. `config.probe.timeout_secs` continues to control the same timeout, just applied once at
client-construction time instead of on every probe call (value is identical across all calls
within a running process either way).

## Risks and Mitigations

- **Risk:** `Client::builder().build()` failing at startup now aborts server startup instead of
  silently degrading.
  **Mitigation:** This is the intended, correct behavior — a client that can't be built (e.g.
  broken TLS backend) should surface immediately as a startup failure, not silently produce a
  probe path with no timeout enforcement. `Client::builder().build()` failures are extremely
  rare in practice (TLS backend initialization only) and this now matches how every other
  startup-time fallible step in `main()` is already handled.
- **Risk:** Sharing one client's connection pool across all probed services could, in theory,
  let connection-pool limits from one slow/many-connection host affect others.
  **Mitigation:** `reqwest::Client`'s default connection pool is per-host keyed (not a single
  shared limit across all hosts), so this is not a practical concern; this is standard,
  recommended `reqwest` usage (the crate's own docs recommend reusing one `Client`) and matches
  the existing `notify_client` precedent already in this codebase.

## Test Plan

`cargo test -p vexboard-server` — no existing test exercises `probe_service`/`start_probe_loop`
directly (no HTTP-mocking test harness in this project), so behavior is unaffected for the
currently-tested paths. No new test added — this is a mechanical refactor (client construction
moved from per-call to once-at-startup) with identical observable HTTP behavior (same timeout
value, same request logic); correctness is verified by `cargo build`/`clippy` type-checking the
threaded-through `&reqwest::Client` parameter across all three call sites compiles and matches
existing usage patterns (`client.head(&url)` / `client.get(&url)` are unchanged reqwest API
calls, just against a passed-in client instead of a locally-built one).
