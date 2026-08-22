# Notification Channel Kinds (Telegram/Gotify) & Alert Rules — Specification

Status: Phase 1 complete, proceeding to Phase 2 implementation.

Covers the two follow-up items the user picked from the deferred list
after Feature 3 (notification channels): more channel kinds, and
failure-threshold / repeat-while-down rules.

## 1. Current state analysis

- `notify.rs::build_notification` matches on `channel.kind.as_str()` with
  arms for `"discord"`, `"ntfy"`, and a fallback treated as `"webhook"`.
  Adding a kind is exactly the one-`match`-arm extension the original
  notifications spec designed for.
- `NotificationChannel` has exactly two channel-specific fields:
  `target: String` and `secret: Option<String>`. Every kind so far maps
  `target` → destination URL and `secret` → optional signing credential.
- `notification_channels.kind` has a DB-level `CHECK(kind IN ('webhook',
  'discord', 'ntfy'))` constraint (migration `010`). SQLite can't alter a
  `CHECK` constraint in place — widening it means the
  recreate-table-and-copy pattern migration `008` already used in this
  codebase for an analogous constraint change.
- `notification_loop` fires exactly on transition (prev status ≠ current
  status), with no concept of "how many times has it been down" or "when
  did we last say something" — a single failed probe alerts immediately,
  and a still-down service never gets a follow-up.
- `db::get_setting`/`set_setting` already provide a generic key/value
  upsert against the `settings` table (used today for `auth_mode`) — no
  schema change needed to add two more keys.

## 2. Scope

### 2a. New channel kinds: Telegram and Gotify — not SMTP

Both Telegram and Gotify are plain HTTP POST + JSON, fitting the existing
`target`/`secret` → `OutgoingNotification` adapter pattern with zero
schema or dependency changes:

- **Telegram**: `target` = chat id, `secret` = bot token (required, not
  optional like the webhook HMAC secret — Telegram has no unsigned mode).
  `POST https://api.telegram.org/bot<token>/sendMessage`, JSON body
  `{"chat_id": ..., "text": ..., "parse_mode": "Markdown"}`.
- **Gotify**: `target` = server base URL, `secret` = app token (required).
  `POST <target>/message`, header `X-Gotify-Key: <token>`, JSON body
  `{"title": "VexBoard", "message": ..., "priority": 8|2}`.

**SMTP is explicitly not included**, discovered during design rather than
silently dropped: it doesn't fit this adapter at all. Every existing and
above kind is an HTTP POST built from `target`+`secret`, deliverable
through the existing `reqwest::Client`-based `send_once`/`send_with_retry`.
SMTP needs an actual mail client (a new dependency — `lettre` is the
standard choice, itself needing a Context7 pass and its own delivery path,
since `OutgoingNotification { url, headers, body }` has no meaning for an
email) plus more configuration than two string fields comfortably hold
(host, port, credentials, TLS mode, from-address). That's a distinctly
separate piece of work, not a `match` arm — left for its own pass rather
than bolted on half-fitting.

`VALID_KINDS`/`VALID_CHANNEL_KINDS` (duplicated today between
`api/notifications.rs` and `api/config_export.rs`'s import validator) both
grow to include `"telegram"`/`"gotify"`. Channel creation/update rejects a
Telegram or Gotify channel with an empty `secret` — for these two kinds
it's a required credential, not an optional signing key.

### 2b. Alert rules: failure threshold + repeat interval

Two new global settings, stored via the existing `settings` key/value
table (not `config.toml` — consistent with this session's whole direction
of moving admin-tunable things into the DB+UI):

- `notify_fail_threshold` (default `1` — preserves today's exact
  behavior: alert on the first failed probe).
- `notify_repeat_interval_mins` (default `0` — preserves today's exact
  behavior: never repeat while still down).

`notification_loop`'s per-service state grows from "last known status"
to:

```rust
struct ServiceAlertState {
    current_status: String,
    consecutive_down: i64,
    notified_down: bool,        // did this outage actually cross the threshold and alert?
    last_notified_at: Option<Instant>,
}
```

On a `"down"` observation: increment `consecutive_down`; fire if not yet
notified and the threshold is now met, or if already notified and
`repeat_interval_mins` has elapsed since the last alert. On an `"up"`
observation: fire a recovery notice **only if this outage actually
alerted** (`notified_down`) — a blip that never crossed the threshold
correctly produces no "back up" message either, since nothing was ever
said about it going down. Reset all per-service state on recovery.

With both settings at their defaults this is byte-for-byte the existing
behavior (threshold 1 fires immediately, interval 0 never repeats) — a
tuning knob added to the existing loop, not a behavior change for anyone
who doesn't touch it.

New admin-only endpoints on the existing notifications router:
`GET /api/v1/notifications/rules`, `PATCH /api/v1/notifications/rules`
(`{fail_threshold, repeat_interval_mins}`, both validated ≥ their floor —
threshold ≥ 1, interval ≥ 0).

### Frontend

A "Rules" card added to the existing Notifications settings pane (already
built in Feature 3): two number inputs, Save, inline confirmation —
matching the pane's existing card/row structure exactly, no new layout
primitives.

## 3. Explicitly out of scope

- SMTP (see 2a).
- Per-service overrides of the threshold/interval (global only, matching
  how `retry_count`/`retry_delay_secs` are global today) — per-service
  tuning is real added value but a distinctly bigger UI/data-model piece,
  not implied by "do 3a and 3c."
- Exposing these two settings through `config/export`'s `ExportedSettings`
  — Feature 5 deliberately keeps `settings` export/import reference-only
  and out of the auto-applied path; extending that surface wasn't asked
  for here and would touch already-shipped, reviewed code for a feature
  that wasn't part of this request.

## 4. Files touched

Backend: `db/migrations/011_notification_channel_kinds.sql` (new),
`db/mod.rs` (migration registration), `notify.rs` (new match arms, new
alert-state loop), `api/notifications.rs` (`VALID_KINDS` widened, secret
required for telegram/gotify, new `rules` endpoints), `api/config_export.rs`
(`VALID_CHANNEL_KINDS` widened only), `api/openapi.rs`, `api/mod.rs` if a
new schema type is needed, `tests.rs`.
Frontend: `pages/settings/notifications.rs` (kind dropdown gains two
options, secret becomes required for them, new Rules card).

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Widening a `CHECK` constraint on SQLite needs a table rebuild | Same recreate-copy-rename pattern already used in migration `008`; idempotency guarded by inspecting `sqlite_master.sql` for `'telegram'` rather than a fragile row-count heuristic |
| Changing threshold/interval defaults would silently alter existing alert behavior | Defaults (`1`, `0`) are chosen to reproduce the exact current behavior; only an admin who explicitly changes them sees different behavior |
| A blip that doesn't cross the threshold still triggers a confusing "recovered" message | `notified_down` gates the recovery notice — no alert in, no alert out |
| Telegram/Gotify created without a token silently fail at delivery time | Rejected at creation/update time with a 400, not discovered later when the first alert fails |

## 6. Approved validation commands

Same as established: `cargo fmt --all -- --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test -p vexboard-server`,
`cargo build --release --bin vexboard-server`, `scripts/preflight.ps1`,
`cargo check`/`clippy --target wasm32-unknown-unknown -p vexboard-frontend`.
