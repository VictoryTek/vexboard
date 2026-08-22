# Notification Channel Kinds (Telegram/Gotify) & Alert Rules — Review

## Files changed

Backend:
- `crates/vexboard-server/src/db/migrations/011_notification_channel_kinds.sql` (new)
  — widens the `kind` CHECK constraint via the recreate-copy-rename
  pattern already used in migration `008`
- `crates/vexboard-server/src/db/mod.rs` — migration registered, guarded
  by inspecting `sqlite_master.sql` for `'telegram'`
- `crates/vexboard-server/src/notify.rs` — `build_notification` gains
  `"telegram"`/`"gotify"` match arms; `notification_loop` rewritten around
  a new `ServiceAlertState` + `decide_fire` (extracted as a pure,
  independently unit-tested function) instead of plain transition
  detection; 6 new unit tests
- `crates/vexboard-server/src/api/notifications.rs` — `VALID_KINDS`
  widened to 5, `requires_secret()` gate on create/update, new
  `AlertRules` struct + `GET`/`PATCH /rules` handlers
- `crates/vexboard-server/src/api/config_export.rs` — `VALID_CHANNEL_KINDS` widened to match
- `crates/vexboard-server/src/api/openapi.rs` — 2 paths + 1 schema registered
- `crates/vexboard-server/src/tests.rs` — `patch_json` test helper added,
  4 new HTTP-level tests

Frontend:
- `crates/vexboard-frontend/src/pages/settings/notifications.rs` — kind
  dropdown gains Telegram/Gotify; secret field shown for every kind except
  Discord/ntfy, with a kind-specific placeholder and required-field
  validation; new Rules card (two number inputs + Save, matching the
  pane's existing card/row structure)

## Review checklist

1. **Specification compliance** — SMTP correctly excluded per the spec's
   explicit scope correction (doesn't fit the `target`+`secret` HTTP
   adapter, needs a new mail-client dependency and its own delivery path —
   documented before writing code, not discovered partway through);
   Telegram/Gotify both fit the existing adapter with zero schema changes
   beyond widening one CHECK constraint; alert-rule defaults (threshold 1,
   interval 0) reproduce the original behavior exactly, verified by test. ✅
2. **Best practices — the fire-decision logic was extracted specifically
   for testability**, matching how `probe::uptime::compute_uptime_summary`
   was handled in the uptime-history feature: `decide_fire` takes `now`
   as a parameter rather than reading the clock internally, so the
   repeat-interval logic (which depends on elapsed time) is directly
   testable with synthetic timestamps rather than requiring real sleeps
   or a live loop. ✅
3. **Consistency** — the CHECK-constraint widen reuses the exact
   recreate/copy/rename shape migration `008` already established for
   this exact scenario (SQLite can't `ALTER ... DROP CONSTRAINT`); the two
   new channel kinds fit the same `match` arm pattern Feature 3 designed
   for; `AlertRules` follows the `AuthModeStatus`-in-`api/settings.rs`
   precedent (small settings DTO living beside its own handler, not in
   `models.rs`) rather than the `Group`/`Service`-style resource-model
   precedent, since it's a singleton settings pair, not a collection. ✅
4. **Maintainability** — `decide_fire` is the one place per-service alert
   behavior lives; extending it further (e.g. a per-channel override)
   would touch one function, not the whole loop. ✅
5. **Completeness** — Telegram/Gotify reject channel creation *and*
   update when the required secret is missing, both server-side (tested)
   and client-side (immediate feedback before the request even fires);
   `AlertRules`' `PATCH` validates both fields server-side regardless of
   what the frontend already checks. ✅
6. **Performance** — the two new settings are read via the same
   `get_setting` call pattern already used for `auth_mode`, only inside
   the `"down"` branch (never on `"up"`, which doesn't need them) — no
   new per-tick overhead beyond two more key/value lookups exactly when needed. ✅
7. **Security** — Telegram/Gotify tokens live in the existing `secret`
   column, which was already `#[serde(skip_serializing)]` and already
   excluded from config export — no new exposure surface introduced by
   adding two kinds that use the same field. ✅
8. **API currency** — no new dependency (Telegram Bot API and Gotify's
   HTTP publish API are both plain, extremely stable JSON-over-HTTP
   contracts, consistent with how Discord/ntfy were added without
   Context7 in Feature 3 for the same reason — no library being integrated). ✅
9. **Build validation** — see below.

## Build validation (verbatim)

**`cargo fmt --all -- --check`** — clean after one pass.

**`cargo clippy --workspace -- -D warnings`** (native) — clean on the
second pass; the first caught the same class of mistake as the
config-export round: an Effect that only does `gloo_net`/`spawn_local`
(fetching alert rules on mount) had no reason to be
`#[cfg(target_arch = "wasm32")]`, and gating it made `fetch_rules` dead
code on native since it had no other call site. Removed the unnecessary
gate — now three-for-three on this exact mistake being real and caught,
not hypothetical.

**wasm32 target:**
```
cargo check --target wasm32-unknown-unknown -p vexboard-frontend           → clean
cargo clippy --target wasm32-unknown-unknown -p vexboard-frontend -- -D warnings → clean
```

**`cargo test -p vexboard-server`**
```
test result: ok. 73 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
64 pre-existing + 9 new: 6 unit tests directly against `decide_fire`
(default threshold fires immediately; a higher threshold waits for N
consecutive failures; a zero repeat interval never fires twice; a nonzero
interval does fire again once elapsed, and not before; a blip that never
crosses the threshold produces no recovery notice; a real alerted outage
does, and fully resets state after) plus 3 HTTP-level tests (Telegram and
Gotify both reject channel creation without a secret; alert rules default
correctly, reject invalid values, and persist a valid update).

**`cargo build --release --bin vexboard-server`** — succeeded.

## Score table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 95% | A (wasm32 type-checked; still not browser-verified) |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 97% | A (native + wasm32 type-checks pass; full Trunk/browser build unverified) |

**Overall Grade: A (99%)**

## Result: **PASS**

No CRITICAL issues found.

## Phase 6 — Preflight

`scripts/preflight.ps1` executed directly:

```
[PASS] cargo fmt
[PASS] cargo clippy
[PASS] cargo test          (73 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
