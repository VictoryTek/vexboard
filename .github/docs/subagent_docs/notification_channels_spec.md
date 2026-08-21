# Notification Channels — Specification

Status: Phase 1 complete, proceeding to Phase 2 implementation.

## 1. Current state analysis

- `notify.rs` already has solid delivery machinery: a background loop
  subscribed to the probe broadcast channel, transition-only firing
  (silences the initial probe at boot), retry-with-backoff, and
  HMAC-SHA256 request signing.
- What it lacks: the destination list is **config-file-only**
  (`config.notifications.webhooks: Vec<WebhookConfig>`, read once at
  startup) and there's exactly one payload shape (a raw JSON webhook).
  Adding or changing a destination means editing `config.toml` and
  restarting the process — the same "backend ahead of UI" gap the
  Settings facelift diagnosed for everything else.
- Every other user-managed collection in this app (services, groups, quick
  links, users) lives in the database with a CRUD API and a UI. Webhooks
  are the one exception, purely historical (they shipped before this
  pattern was established).
- Groups (`api/groups.rs`) is a clean, minimal CRUD template already in
  this codebase — full list/create/update/delete against a simple table,
  audit-logged, `require_admin`-gated — and is the template this feature follows.

## 2. Problem definition

Let an admin add a real notification destination — Discord, ntfy, or a raw
webhook — from the Settings UI, test it immediately, and manage it without
touching a config file or restarting the server.

## 3. Scope

**In scope**, sized to match the Uptime History and Service Control passes:

- Move destinations from `config.notifications.webhooks` into a new
  `notification_channels` DB table with full CRUD + a `test` action.
- Three channel kinds as payload adapters over the existing delivery
  loop: `webhook` (today's raw-JSON + HMAC behavior, generalized),
  `discord` (Discord webhook `{"content": ...}`), `ntfy` (ntfy's
  plain-text publish API with `Title`/`Priority`/`Tags` headers).
- A real "Notifications" tab in Settings (the earlier facelift
  deliberately left this out — nothing backed it yet).

**Explicitly deferred** (same reasoning as prior features: ship the
tractable slice now, note what's next rather than block on it):

- Telegram / Gotify / SMTP as additional kinds — same adapter pattern
  (`build_notification` gets one more `match` arm each), no architecture
  change needed, just more payload shapes.
- Per-service routing (today, and after this change: an event for *any*
  service goes to *every* enabled channel whose event filter matches).
  Event-type filtering already covers the common "only tell me about
  outages" case without a many-to-many channel↔service table.
- Re-notify interval / failure-threshold "rules" — today's transition-only
  firing is already not spammy on its own; a repeat-while-still-down
  option is a separate, additive feature.
- Maintenance windows — distinct scheduling UI and scope, not blocking this.

Two channel kinds beyond the original `webhook` were chosen — not all
four pitched — specifically to prove the adapter pattern generalizes
without one PR trying to be four integrations at once.

## 4. Design

### 4a. Database

New migration `010_notification_channels.sql`, following the exact
`CREATE TABLE IF NOT EXISTS` pattern already used for `audit_log` /
`dismissed_units`:

```sql
CREATE TABLE IF NOT EXISTS notification_channels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('webhook', 'discord', 'ntfy')),
    target      TEXT NOT NULL,
    secret      TEXT,
    events      TEXT NOT NULL DEFAULT '[]',
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

`target` is always a single URL to POST to (the Discord webhook URL, the
full ntfy topic URL including its own base — self-hosted ntfy just means a
different base — or a generic webhook endpoint), keeping every kind
uniform rather than growing kind-specific columns. `events` is a JSON
array stored as text, matching the existing convention for
`services.tags` (`Service.tags: Option<String>`, raw JSON exposed as-is
to the frontend, which decodes it there) — followed here for consistency
rather than introducing a new server-side-array-parsing convention this
codebase doesn't otherwise use.

`config.notifications.webhook_secret` / `retry_count` / `retry_delay_secs`
**stay in config.toml** — they're global delivery tuning, rarely touched,
the same category as `probe.timeout_secs`. Only the destination *list*
moves to the database. `NotificationsConfig.webhooks: Vec<WebhookConfig>`
and the `WebhookConfig` struct are deleted; a leftover
`[[notifications.webhooks]]` block in an existing `config.toml` is
silently ignored by the `config` crate (no `deny_unknown_fields`) rather
than erroring — same acceptable-impact reasoning as the
`max_history`→`history_retention_days` rename in Feature 1 (pre-1.0, no
UI ever existed for this, real-world impact is limited to anyone who
hand-wrote the TOML).

### 4b. Models (`db/models.rs`, mirroring `Group`/`CreateGroup`/`UpdateGroup`)

```rust
pub struct NotificationChannel {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub target: String,
    #[serde(skip_serializing)]   // write-only, like a password — never round-tripped
    pub secret: Option<String>,
    pub events: String,          // JSON array as text, see 4a
    pub enabled: bool,
    pub created_at: Option<NaiveDateTime>,
}

pub struct CreateNotificationChannel { name, kind, target, secret: Option<String>, events: Vec<String> }
pub struct UpdateNotificationChannel { name, kind, target, secret (nullable-clearing via the
    existing `deserialize_some` helper, same pattern as `UpdateGroup.icon`), events, enabled }
```

### 4c. API (`api/notifications.rs`, new — entirely admin-gated)

Every route here is admin-only, with no read tier for viewers — unlike
services/groups, a channel's `target` can itself function as a bearer
credential (anyone with a Discord webhook URL can post to it), so this
isn't information to expose to non-admins at all.

```
GET    /api/v1/notifications/channels
POST   /api/v1/notifications/channels
PATCH  /api/v1/notifications/channels/{id}
DELETE /api/v1/notifications/channels/{id}
POST   /api/v1/notifications/channels/{id}/test
```

CRUD handlers follow `api/groups.rs` almost line for line (fetch-existing/
merge/update pattern for PATCH, audit-log every mutation). `test` fetches
the channel, builds a synthetic "test" notification via the same adapter
used for real events, fires **one** delivery attempt (no retry — a test
button needs an immediate yes/no, not a 30-second wait), and returns the
real outcome (`{"status": "ok"}` or `{"error": "<what actually failed>"}`)
so the UI can show it inline rather than optimistically assuming success.

### 4d. Delivery (`notify.rs`)

Split "what to send" from "how to send it reliably":

```rust
pub struct OutgoingNotification { pub url: String, pub headers: Vec<(String, String)>, pub body: String }

pub fn build_notification(
    channel: &NotificationChannel,
    event: &ProbeEvent,
    event_type: &str,
    previous_status: Option<&str>,
    config: &NotificationsConfig,
) -> OutgoingNotification
```

One `match` on `channel.kind`:
- `"discord"` → JSON `{"content": "🔴/🟢 **{service_name}** is {status}"}`
- `"ntfy"` → plain-text body `"{service_name} is {status}"`, `Title`/
  `Priority`/`Tags` headers (`high`/`warning` for down, `default`/
  `white_check_mark` for up)
- anything else (`"webhook"`) → the **same JSON shape the current code
  already sends** (`event`, `service_id`, `service_name`, `status`,
  `previous_status`, `url`, `latency_ms`, `timestamp`), plus the existing
  `X-Webhook-Signature` HMAC header when a secret is set (per-channel
  `secret`, falling back to the global `config.webhook_secret`) — kept
  byte-for-byte compatible so an existing downstream consumer parsing this
  payload doesn't need to change, only where the destination is configured

`send_once(client, notification) -> Result<(), String>` fires one HTTP
POST and reports the real outcome — used directly by the `test` endpoint.
`send_with_retry(...)` wraps it with the existing backoff loop — used by
the background `notification_loop`, which now queries
`notification_channels WHERE enabled = 1` per transition event instead of
reading `config.webhooks` once at startup. `notification_loop` gains a
`db: SqlitePool` parameter; its one call site in `main.rs` passes `db.clone()`.

### 4e. Frontend

New `pages/settings/notifications.rs`, added as a "Notifications" tab in
the Administration group (the rail/pane shell built in the Settings
facelift already supports adding a section in ~30 lines — this is exactly
the kind of pane that work was building toward). Per channel: name, a kind
badge, an enabled toggle-equivalent (reusing the existing
`.settings-nav-option`-style picker isn't right here — a plain "Enabled"/
"Disabled" pill with a click-to-toggle PATCH is simpler and matches this
pane's actual need), Test and Delete buttons. An "Add channel" form:
name, kind `<select>`, target URL, secret (shown only for `webhook`),
event-filter checkboxes (Down / Up, empty = both). Test button shows the
real inline result (success or the actual error text), matching the
History modal's control-action pattern from Feature 2 rather than a
silent fire-and-forget.

## 5. Dependencies

None new. No Context7 lookup for Discord's webhook payload or ntfy's
publish API — both are plain HTTP contracts sent via the `reqwest::Client`
already in `AppState`, not a new Rust crate being integrated; Context7's
policy is aimed at library/framework APIs, and there's no library here to
version-check. (This is a deliberate scoping call, stated rather than
silently skipped.)

## 6. Files touched

Backend: `db/migrations/010_notification_channels.sql` (new), `db/mod.rs`
(register migration), `db/models.rs`, `api/notifications.rs` (new),
`api/mod.rs` (mount router), `api/openapi.rs`, `config.rs`
(remove `WebhookConfig`/`webhooks`), `config/default.toml`, `notify.rs`
(adapters + `db` param), `main.rs` (pass `db.clone()` to
`notification_loop`), `README.md` (update the stale Nix example).
Frontend: `pages/settings/notifications.rs` (new), `pages/settings/mod.rs`
(register tab).

## 7. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Removing `webhooks` from config silently drops an existing deployment's alerts | No UI ever existed for this before; anyone affected re-adds through the new UI once, a one-time cost, documented in the commit/README |
| A malformed `target` URL for Discord/ntfy fails silently in the background loop | The Test button gives immediate, real feedback before an admin walks away trusting a broken channel |
| `secret` leaking via API responses | `#[serde(skip_serializing)]`, same treatment as user password hashes elsewhere in this codebase |
| Channel `target` is itself a bearer credential | No read tier for non-admins at all (stricter than services/groups, which do have public read routes) |

## 8. Approved validation commands

Same as established: `cargo fmt --all -- --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test -p vexboard-server`,
`cargo build --release --bin vexboard-server`, `scripts/preflight.ps1`.
