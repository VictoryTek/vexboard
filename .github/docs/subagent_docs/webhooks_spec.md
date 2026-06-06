# Webhook / Notification Support — Specification
**Phase:** 1 — Research & Specification
**Date:** 2026-06-05
**Feature:** Feature Recommendation #7 from project_audit_2026-06-04

---

## 1. Current State Analysis

The probe subsystem already broadcasts `ProbeEvent` values onto `probe_tx: broadcast::Sender<ProbeEvent>`.
`AppState` holds a reference to that sender, but no code ever subscribes to the receiver for anything
other than keeping the channel alive. `ProbeEvent` carries only `service_id`, `status`, and `latency_ms`.

The `reqwest` HTTP client crate is already a workspace dependency (v0.13.1) and is used inside
`probe/uptime.rs` for URL probing. `sha2 = "0.10.9"` and `hmac = "0.12.1"` are already compiled as
transitive dependencies (visible in `Cargo.lock`) but are not declared as direct deps.

No `[notifications]` configuration section exists. No webhook delivery code exists anywhere.

### Files affected:
- `crates/vexboard-server/src/probe/uptime.rs` — extend `ProbeEvent` with service metadata
- **NEW** `crates/vexboard-server/src/notify.rs` — webhook state tracker + delivery loop
- `crates/vexboard-server/src/config.rs` — add `NotificationsConfig` + `WebhookConfig`
- `config/default.toml` — add `[notifications]` section (documented example, disabled by default)
- `crates/vexboard-server/src/main.rs` — declare `mod notify`, spawn notification loop
- `crates/vexboard-server/Cargo.toml` — add `sha2 = "0.10"`, `hmac = "0.12"`

---

## 2. Problem Definition

When a probed service transitions from `up` to `down` (or recovers to `up`), there is no mechanism to
alert the operator outside the browser dashboard. VexBoard is useful for passive monitoring but not for
on-call workflows where alerting on state changes is essential.

---

## 3. Proposed Solution Architecture

### 3.1 ProbeEvent Extension (`probe/uptime.rs`)

Add `service_name: String` and `url: Option<String>` to `ProbeEvent`. The `probe_service()` function
already receives `svc: &Service`, so these fields can be populated without any new DB query.

```rust
pub struct ProbeEvent {
    pub service_id: i64,
    pub service_name: String,   // new
    pub url: Option<String>,    // new
    pub status: String,
    pub latency_ms: Option<i64>,
}
```

This avoids a secondary DB round-trip per event in the notification loop.

### 3.2 Configuration (`config.rs` + `config/default.toml`)

New structs:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    /// Subset of event types to deliver. Empty = deliver all events.
    /// Supported: "service.down", "service.up"
    #[serde(default)]
    pub events: Vec<String>,
    /// Per-webhook HMAC secret (overrides the global webhook_secret when set)
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NotificationsConfig {
    /// Global HMAC-SHA256 signing secret. Applied to webhooks that do not set their own secret.
    /// Leave empty to disable signing.
    #[serde(default)]
    pub webhook_secret: String,
    /// Number of retries after an initial delivery failure (default 2).
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// Base delay in seconds between retries; multiplied by attempt number (default 2).
    #[serde(default = "default_retry_delay_secs")]
    pub retry_delay_secs: u64,
    /// Webhook endpoint configurations.
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
}
```

Added to `AppConfig` with `#[serde(default)]` so an absent `[notifications]` section does not fail
deserialization (zero webhooks = loop exits immediately / is skipped):

```rust
#[serde(default)]
pub notifications: NotificationsConfig,
```

Default config section added to `config/default.toml` (as comments — no webhooks enabled by default):

```toml
[notifications]
# HMAC-SHA256 signing secret applied to all webhooks (leave empty to skip signing).
webhook_secret = ""
# Retry settings for failed webhook deliveries.
retry_count = 2
retry_delay_secs = 2
# Define webhook endpoints below.
# [[notifications.webhooks]]
# url = "https://hooks.example.com/vexboard"
# events = ["service.down", "service.up"]  # omit to receive all events
# secret = ""  # per-webhook secret override
```

### 3.3 Notification Loop (`notify.rs`)

Single public async function `notification_loop` runs in a spawned Tokio task:

```
probe_rx.recv()
   │
   ▼
prev_status lookup (HashMap<i64, String>)
   │
   ├── None (first probe for this service) → skip, record status
   ├── Same as current → skip, no transition
   └── Different → state transition detected
                      │
                      ▼
                  Determine event type:
                    "service.down" (up → down)
                    "service.up"   (down → up)
                      │
                      ▼
                  Build JSON payload
                      │
                      ▼
                  For each webhook:
                    ├── apply event filter
                    ├── resolve signing secret
                    └── fire_webhook() with retry
```

**Payload schema:**
```json
{
  "event": "service.down",
  "service_id": 42,
  "service_name": "Gitea",
  "status": "down",
  "previous_status": "up",
  "url": "https://git.example.com",
  "latency_ms": null,
  "timestamp": "2026-06-05T12:34:56.789Z"
}
```

**Signing:** If a secret is set, the HMAC-SHA256 digest of the serialized payload body is sent as:
`X-Webhook-Signature: sha256=<hex_digest>`

HMAC implementation uses `hmac` + `sha2` crates (already compiled as transitive deps; added as direct
deps in `Cargo.toml`). Hex encoding is done inline without adding a `hex` crate dependency.

**Retry logic:**
- Attempt 0: immediate delivery
- On failure (non-2xx response or network error): sleep `retry_delay_secs * attempt` seconds, retry
- Maximum `retry_count` additional attempts (default 2 → up to 3 total delivery attempts)
- Final failure logged at ERROR; intermediate failures at WARN

**Startup behaviour:**
- The first probe result for any service is silently recorded in `prev_status` without firing webhooks.
  This prevents a flood of `service.up` alerts when VexBoard restarts.

**No webhooks configured:**
- If `config.notifications.webhooks` is empty, the loop still runs but exits the delivery loop
  immediately for every event. This is essentially zero overhead — the broadcast receiver must
  exist to prevent the channel from being considered idle.

### 3.4 `main.rs` wiring

After spawning the probe loop:
```rust
let notify_config = config.notifications.clone();
let notify_rx = probe_tx.subscribe();
let notify_client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .unwrap_or_default();
tokio::spawn(async move {
    notify::notification_loop(notify_rx, notify_config, notify_client).await;
});
```

The `reqwest::Client` is created once and reused for all webhook deliveries (connection pooling).

---

## 4. Implementation Steps

1. Extend `ProbeEvent` in `probe/uptime.rs` with `service_name` and `url` fields.
2. Update the `probe_service()` call site that constructs `ProbeEvent` to populate new fields.
3. Add `sha2 = "0.10"`, `hmac = "0.12"` to `crates/vexboard-server/Cargo.toml`.
4. Add `WebhookConfig`, `NotificationsConfig` to `config.rs`.
5. Add `#[serde(default)] pub notifications: NotificationsConfig` to `AppConfig`.
6. Add `[notifications]` section (commented) to `config/default.toml`.
7. Create `crates/vexboard-server/src/notify.rs` with `notification_loop` and `fire_webhook`.
8. Add `mod notify` to `main.rs`, spawn the notification task after the probe loop task.

---

## 5. Dependencies

| Crate | Version | Status | Reason |
|---|---|---|---|
| `reqwest` | 0.13.1 | already direct dep | HTTP delivery of webhooks |
| `sha2` | 0.10 | in lock file (transitive); add as direct dep | HMAC payload signing |
| `hmac` | 0.12 | in lock file (transitive); add as direct dep | HMAC payload signing |

No new crates added to `Cargo.lock`. No network fetch required.

Context7 verification: NOT required — no new external crates added.

---

## 6. Configuration Changes

`config/default.toml` gains a `[notifications]` section with all keys commented out.
Deserialization with `#[serde(default)]` means existing deployments that omit the section continue
to work unchanged.

---

## 7. Build and Test Commands (Phase 3)

| Command | Purpose |
|---|---|
| `cargo fmt --all -- --check` | Formatting |
| `cargo clippy --workspace -- -D warnings` | Lint |
| `cargo build --release --bin vexboard-server` | Backend binary compiles |
| `scripts/preflight.sh` | Full gate (covers all above + tests) |

---

## 8. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Webhook delivery blocks the notification loop | Use `tokio::spawn` per webhook delivery attempt — each fires independently |
| Service goes down repeatedly, flooding webhook | Only fire on transitions; repeated `down` after `down` is suppressed |
| Startup `service.up` flood | First probe per service is silently recorded, not delivered |
| Network errors cause loop crash | All `reqwest` errors are caught and logged; loop continues |
| ProbeEvent schema change breaks SSE subscribers | `probe_tx` is not subscribed to by any SSE handler (verified by grep) — only the notification loop subscribes |
| `sha2`/`hmac` version mismatch with existing transitive deps | Using the exact versions already in `Cargo.lock` (0.10 / 0.12) |

---

## 9. File Inventory

Files to be modified:
- `crates/vexboard-server/src/probe/uptime.rs`
- `crates/vexboard-server/src/config.rs`
- `config/default.toml`
- `crates/vexboard-server/src/main.rs`
- `crates/vexboard-server/Cargo.toml`

Files to be created:
- `crates/vexboard-server/src/notify.rs`
- `.github/docs/subagent_docs/webhooks_spec.md` (this file)
- `.github/docs/subagent_docs/webhooks_review.md` (Phase 3)
