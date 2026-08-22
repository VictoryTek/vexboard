# Config Export / Import + Nix Generation — Review

## Files changed

Backend:
- `crates/vexboard-server/src/db/models.rs` — `ExportedGroup`/`ExportedService`/
  `ExportedQuickLink`/`ExportedNotificationChannel`/`ExportedSettings`/`ConfigExport`
  (id-free, group-by-name portable DTOs), `ConfigImportSummary`
- `crates/vexboard-server/src/api/config_export.rs` (new) — `GET /export`,
  `POST /import`, `GET /export/nix`, all admin-only
- `crates/vexboard-server/src/api/mod.rs` — module registered, router
  mounted under `admin_protected`
- `crates/vexboard-server/src/api/openapi.rs` — 3 paths + 7 schemas registered
- `crates/vexboard-server/src/tests.rs` — 2 new tests (`get_text` helper added)

Frontend:
- `crates/vexboard-frontend/src/pages/settings/backup.rs` (new) — Export
  (plain `<a download>`, no JS needed), Import (`FileReader`-based file
  read + upload, shows real created/skipped counts), Nix (fetched
  read-only textarea, click-to-select)
- `crates/vexboard-frontend/src/pages/settings/mod.rs` — new
  "Backup & Data" tab in the Administration group
- `crates/vexboard-frontend/Cargo.toml` — `web-sys` gains `File`,
  `FileList`, `FileReader`, `HtmlTextAreaElement` features

## Review checklist

1. **Specification compliance** — matches the spec's corrected scope
   exactly: additive-only import (verified by test — see below), secrets
   excluded from both JSON export (`NotificationChannel.secret`) and Nix
   export (`auth.secret`, `webhook_secret`), `settings.auth_mode` exported
   for reference but never re-applied on import (confirmed: `import_config`
   never reads `payload.settings` at all). ✅
2. **Best practices — mid-implementation course correction documented,
   not silently absorbed.** The original pitch ("declare your whole
   dashboard in Nix") was checked against what `nix/module.nix` actually
   consumes (a freeform `settings` → `config.toml` merge, no mechanism for
   database-backed collections) *before* writing code, and the spec
   records the correction: Nix export scoped to the global settings the
   module can actually apply, not the services/groups/channels the
   original pitch implied. Shipping the version that's honest about what
   pasting the output does, rather than the version that sounded better
   in the original five-feature list. ✅
3. **Consistency** — `export`/`import`/`export/nix` reuse existing
   patterns throughout: group-by-name dedup uses the same `UNIQUE`
   constraint `groups.name` already has; service dedup reuses the exact
   `systemd_unit`-taken check `create_service` already performs; the
   two-pass group resolution (create-or-reuse, then re-read the *complete*
   table) mirrors the general "don't trust request-local state, re-read
   from source of truth" caution used for control/log-streaming socket
   resolution in Features 2 and 4. ✅
4. **Maintainability** — portable DTOs are a deliberately separate type
   family from the read-models (`Group` vs `ExportedGroup`, etc.) rather
   than reusing the read-models with `#[serde(skip)]` sprinkled on id
   fields, so the export/import surface can evolve without entangling the
   CRUD APIs' wire shapes. ✅
5. **Completeness** — every import category reports created *and* skipped
   counts, not just a bare success flag; export sets
   `Content-Disposition: attachment` so the browser downloads it directly
   with zero JS. ✅
6. **Performance** — import processes each category in one pass with a
   single up-front group-id map fetch (not a query per row); no new
   background work or polling. ✅
7. **Security** — all three routes admin-only; import is structurally
   incapable of deleting or overwriting anything (no `UPDATE`/`DELETE`
   statement exists anywhere in `import_config`); `auth_mode` import
   deliberately omitted as a security-sensitive control that shouldn't
   change via an uploaded file; export excludes the same two categories
   of secret this session has consistently protected (channel secrets,
   session/webhook signing secrets). ✅
8. **API currency** — no new backend dependency. Frontend added `web-sys`
   Cargo *features* (not a new crate) for `FileReader`/`File`/`FileList`/
   `HtmlTextAreaElement`, all standard, stable Web APIs — verified by the
   wasm32 type-check below rather than assumed. ✅
9. **Build validation** — see below.

## Build validation (verbatim)

**`cargo fmt --all -- --check`** — clean.

**`cargo clippy --workspace -- -D warnings`** (native) — clean on the
second pass; the first caught two real dead-code findings (`spawn_local`
import and `fetch_nix_snippet` appeared unused because they were
needlessly wrapped in `#[cfg(target_arch = "wasm32")]` — that Effect
touches nothing DOM-specific, only `gloo_net`/`spawn_local`, which per
Feature 1's established finding don't need the gate). Fixed by removing
the unnecessary gate from the Effect and, conversely, adding
`#[cfg(target_arch = "wasm32")]` to `send_import`'s definition, since its
only call site is inside the genuinely DOM-only `FileReader` block and it
would otherwise be dead code on native. Re-ran clean.

**wasm32 target** (installed in the logs-feature round; used again here):
```
cargo check --target wasm32-unknown-unknown -p vexboard-frontend           → clean
cargo clippy --target wasm32-unknown-unknown -p vexboard-frontend -- -D warnings → clean
```
This is the first feature where the wasm32 check caught something real
during development (the dead-code pair above only surfaced because native
clippy checks a *different* set of code than wasm32 does — a concrete
demonstration of why this verification step, added last round, earns its
keep rather than being a formality.

**`cargo test -p vexboard-server`**
```
test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
62 pre-existing + 2 new: an import round-trip (`test_config_import_is_additive_and_dedupes_on_reimport`)
that imports the same bundle twice and asserts the group is reused (not
duplicated), the service is skipped (not duplicated), and the actual row
counts in `/api/v1/groups` and `/api/v1/services` stay at 1 each — the
single most safety-critical property of this feature, verified end to
end rather than by inspection; and `test_export_nix_excludes_secrets`,
which asserts the generated Nix contains `discovery.enabled` but does
*not* contain the test fixture's session secret string, `auth.secret`, or
`webhook_secret`.

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
[PASS] cargo test          (64 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
