# Notification Channels — Review

## Files changed

Backend:
- `crates/vexboard-server/src/db/migrations/010_notification_channels.sql` (new)
- `crates/vexboard-server/src/db/mod.rs` — migration registered
- `crates/vexboard-server/src/db/models.rs` — `NotificationChannel`,
  `CreateNotificationChannel`, `UpdateNotificationChannel`
- `crates/vexboard-server/src/api/notifications.rs` (new) — full CRUD + `test`,
  entirely admin-gated (no read tier), mirrors `api/groups.rs`'s structure
- `crates/vexboard-server/src/api/mod.rs` — module + router mounted under `admin_protected`
- `crates/vexboard-server/src/api/openapi.rs` — 5 paths + 3 schemas registered
- `crates/vexboard-server/src/config.rs` — removed `WebhookConfig` struct
  and `NotificationsConfig.webhooks`; kept `webhook_secret`/`retry_count`/`retry_delay_secs`
- `crates/vexboard-server/src/notify.rs` — rewritten: `OutgoingNotification`
  + `build_notification` (3-kind adapter) + `send_once`/`send_with_retry`
  split; `notification_loop` now queries `notification_channels` from the
  DB per transition instead of reading config once at startup
- `crates/vexboard-server/src/main.rs` — `notification_loop` call site passes `db.clone()`
- `crates/vexboard-server/src/tests.rs` — 3 new tests
- `config/default.toml`, `README.md` — stale webhook-list examples updated

Frontend:
- `crates/vexboard-frontend/src/pages/settings/notifications.rs` (new) —
  list + add-destination form + per-row Test/Enable-Disable/Remove
- `crates/vexboard-frontend/src/pages/settings/mod.rs` — new
  "Notifications" tab in the Administration group
- `crates/vexboard-frontend/style/main.css` — one new rule, `.settings-form-success`

## Review checklist

1. **Specification compliance** — scope matches spec exactly: 3 channel
   kinds (not all 4-5 pitched), DB-backed destinations with config staying
   for global delivery tuning only, full CRUD + Test, real Settings tab.
   Explicitly-deferred items (more kinds, per-service routing, re-notify
   rules, maintenance windows) untouched, as planned. ✅
2. **Best practices** — split "what to send" (`build_notification`, pure,
   no I/O) from "how to send it reliably" (`send_once`/`send_with_retry`),
   so the Test button and the background loop share the exact same
   delivery code path rather than two implementations that could drift. ✅
3. **Consistency** — `api/notifications.rs` follows `api/groups.rs`'s CRUD
   shape near line-for-line (fetch-existing/merge-with-`unwrap_or`/audit
   pattern); `NotificationChannel`/`Create*`/`Update*` follow the
   `Group`/`CreateGroup`/`UpdateGroup` split exactly, including reusing the
   existing `deserialize_some` helper for nullable-clearing `secret`
   updates; the `webhook` payload shape is preserved byte-for-byte from
   the original implementation so no downstream consumer needs to change;
   frontend reuses `.settings-user-row`/`.settings-add-user`/
   `.settings-role-badge` etc. as the generic structural primitives they
   actually are (verified their CSS has no user-specific properties)
   rather than duplicating near-identical rules. ✅
4. **Maintainability** — one `match` per channel kind in
   `build_notification` is where a 4th/5th kind (Telegram, Gotify, SMTP)
   would go — no other architecture change needed to extend this later,
   which was the explicit design goal. ✅
5. **Completeness** — every mutation (create/update/delete/toggle) is
   audit-logged; `secret` is `#[serde(skip_serializing)]` and verified
   absent from list responses by test; Test button surfaces the real
   delivery outcome rather than assuming success. ✅
6. **Performance** — one DB query per transition event (channels are
   fetched fresh, not cached) — acceptable at homelab scale and explicitly
   chosen over cache-invalidation complexity for a rare event; no new
   polling. ✅
7. **Security** — entire `notifications` router sits under
   `admin_protected` (no viewer read route was added at all, stricter than
   services/groups, because a channel's `target` can itself function as a
   bearer credential); `secret` never serialized back to any client. ✅
8. **API currency** — no new crate dependency; Discord/ntfy are plain HTTP
   contracts sent via the existing `reqwest::Client`, not a library
   integration, so Context7 wasn't applicable here — stated explicitly in
   the spec rather than silently skipped. ✅
9. **Build validation** — see below.

## Build validation (verbatim)

**`cargo fmt --all -- --check`** — clean.

**`cargo clippy --workspace -- -D warnings`** — clean, both crates, zero warnings.

**`cargo test -p vexboard-server`**
```
test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
57 pre-existing + 3 new: invalid-`kind` rejection (400), a create→list→delete
round trip that also asserts `secret` never appears in the list response,
and a 404 for testing an unknown channel. Matches this codebase's existing
testing convention of targeting validation/edge-case paths rather than
exhaustively testing every CRUD handler (`api/groups.rs`, the template this
feature follows, has no dedicated tests of its own either).

**`cargo build --release --bin vexboard-server`** — succeeded.

## WASM/Trunk build — not run

Same environment limitation as every feature so far: no
`wasm32-unknown-unknown` target / Trunk on PATH. The new Notifications
settings pane (add-destination form, per-row Test/Enable/Remove, inline
success/error messaging) has not been exercised in an actual browser.

## Score table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 90% | A- (not verified in-browser) |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 95% | A (native checks pass; WASM build unverified) |

**Overall Grade: A (98%)**

## Result: **PASS**

No CRITICAL issues found.

## Phase 6 — Preflight

`scripts/preflight.ps1` executed directly:

```
[PASS] cargo fmt
[PASS] cargo clippy
[PASS] cargo test          (60 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
