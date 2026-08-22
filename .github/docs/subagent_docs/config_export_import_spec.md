# Config Export / Import + Nix Generation — Specification

Status: Phase 1 complete, proceeding to Phase 2 implementation.

## 1. Current state analysis

- Every admin-managed collection (`groups`, `services`, `quick_links`,
  `notification_channels`) lives in the database with a CRUD API. There is
  no way to move that state between instances, or to snapshot it, without
  hand-copying the SQLite file.
- `groups.name` is already `UNIQUE` at the DB level (`001_init.sql`);
  `services.systemd_unit` has a partial unique index
  (`007_unique_systemd_unit.sql`, `WHERE systemd_unit IS NOT NULL`) —
  both usable as natural dedup keys for an importer. `quick_links` and
  `notification_channels` have no uniqueness constraint.
- `nix/module.nix` generates `/etc/vexboard/config.toml` by merging a
  freeform `settings` Nix attrset (`pkgs.formats.toml`) into the module's
  own base config — confirmed during the earlier settings-facelift and
  notifications work. **This freeform mechanism only ever maps to
  `config.toml` keys** (`server`, `auth`, `discovery`, `docker`, `probe`,
  `metrics`, `notifications` delivery tuning). It has no mechanism to
  declare database-backed collections — services, groups, quick links,
  channels — because `config.toml` never described them to begin with;
  they're pure runtime state.

## 2. Correction to the original pitch

The original five-feature pitch imagined "declare your whole dashboard in
Nix." Having now confirmed exactly what `nix/module.nix` actually
consumes, that isn't achievable without extending the module itself with
an activation-time import step (a NixOS module change, unverifiable in
this environment — no Nix installed) — a meaningfully different, larger
piece of work than "export/import." Rather than generate Nix syntax that
implies "paste this and your services appear," which the module doesn't
actually support today, this feature ships the version that's honest and
immediately useful:

1. **Full JSON export/import** for the actual database-backed collections
   — genuine backup/restore and instance-to-instance cloning.
2. **A Nix snippet generator scoped to what the module already consumes**
   — the global `config.toml`-shaped settings (discovery/docker/probe/
   metrics/notification tuning) — letting an admin who's been configuring
   things ad hoc (env vars, manual `config.toml` edits) capture their
   current effective settings as ready-to-paste, *actually-functional* Nix.

This is discovered-during-design scope correction, not an open question —
documented here rather than silently building the misleading version.

## 3. Scope

**In scope:**
- `GET /api/v1/config/export` (admin-only) — groups, services, quick
  links, notification channels as a portable JSON document. Rows
  reference groups **by name**, not numeric id (ids aren't portable
  across instances). `NotificationChannel.secret` is never included —
  same reasoning as user passwords: a shareable backup file shouldn't
  carry a credential. `Content-Disposition: attachment` so a browser GET
  downloads it directly.
- `POST /api/v1/config/import` (admin-only) — **additive only**, never
  destructive:
  - Groups: reuse by name if one already exists (the DB's own `UNIQUE`
    constraint on `groups.name` makes this the natural key); otherwise create.
  - Services with a `systemd_unit`: skip (and count) if that unit is
    already claimed by any service — same rule `create_service` already
    enforces; otherwise create.
  - Services without a `systemd_unit` (manual/URL-only): always created —
    there's no natural dedup key, and a duplicate manual entry is a minor,
    correctable annoyance, not a safety problem.
  - Quick links and notification channels: always created (no
    uniqueness constraint exists for either today).
  - **`settings.auth_mode` is exported for reference but never applied on
    import.** Silently changing whether the whole dashboard requires
    login, driven by an uploaded file, is a real risk for a control this
    security-sensitive; the admin changes it explicitly via the Security
    tab, never via import. Stated plainly rather than silently doing the
    "convenient" thing.
  - Returns a per-category created/skipped count so the admin sees
    exactly what happened — the tractable version of "you'll see what
    changes," short of a full pre-commit diff UI (deferred, see below).
- `GET /api/v1/config/export/nix` (admin-only) — the current effective
  `discovery`/`docker`/`probe`/`metrics`/`notifications` (retry tuning
  only) settings, plus the non-secret half of `auth`
  (`secure_cookies`/rate-limit/`behind_proxy`), rendered as a
  `services.vexboard.settings = { ... };` Nix attrset matching the
  README's documented shape exactly. **Never includes `auth.secret` or
  `notifications.webhook_secret`** — those are credentials, and the
  README's own convention keeps secrets out of Nix source entirely
  (`secretFile`, a separate file path). Plain text response; the frontend
  offers a Copy button.
- Frontend: the "Backup & Data" Settings tab sketched in the original
  facelift mockup but never built (nothing backed it then) — Export
  (download), Import (choose file, upload, show the real created/skipped
  counts), Copy as Nix.

**Explicitly deferred**, continuing the pattern from every prior feature:
- A destructive "replace everything" import mode, and any pre-commit
  diff/preview UI — meaningfully higher risk (can wipe a live dashboard)
  for marginal gain over additive-only, which already covers the two real
  use cases (backup/restore into a fresh instance, cloning a setup).
- Declaring services/groups/quick-links/channels via the Nix module
  itself — would require an activation-time import step in
  `nix/module.nix`, unverifiable here without Nix installed, and a
  distinctly different piece of work than export/import.
- Importing users — password hashes are exactly the kind of secret that
  shouldn't round-trip through a shareable file, and a fresh instance's
  setup flow already creates its own admin.

## 4. Design

### 4a. Models (`db/models.rs`)

Portable, id-free DTOs — separate from the existing `Group`/`Service`/
`QuickLink`/`NotificationChannel` read-models, which carry instance-local
ids and timestamps that don't belong in a portable document:

```rust
pub struct ConfigExport {
    pub version: u32,           // 1
    pub exported_at: String,    // RFC3339
    pub groups: Vec<ExportedGroup>,
    pub services: Vec<ExportedService>,
    pub quick_links: Vec<ExportedQuickLink>,
    pub notification_channels: Vec<ExportedNotificationChannel>,
    pub settings: ExportedSettings,
}
pub struct ExportedGroup { name, icon, color, sort_order }
pub struct ExportedService {
    systemd_unit, discovery_source, display_name, description, url, icon,
    group_name: Option<String>, sort_order, probe_enabled, probe_interval,
    tags: Option<Vec<String>>, visible, skip_tls_verify,
}
pub struct ExportedQuickLink { title, url, icon, description, group_name: Option<String>, sort_order }
pub struct ExportedNotificationChannel { name, kind, target, events: Vec<String>, enabled }  // no secret
pub struct ExportedSettings { auth_mode: Option<String> }  // reference only, not re-applied
```

### 4b. API (`api/config_export.rs`, new — all three routes admin-only)

```
GET  /api/v1/config/export        → ConfigExport JSON, download headers
POST /api/v1/config/import        → ConfigExport JSON body → created/skipped summary
GET  /api/v1/config/export/nix    → text/plain Nix attrset
```

Import resolves group references in two passes: first create-or-reuse
every group in the bundle (by name), then re-read the *complete* current
`groups` table into a `name → id` map (not just the ones from this
import) so a service/quick-link referencing a group that already existed
outside this bundle still resolves correctly; an unresolvable name yields
`group_id = NULL` rather than failing the whole import.

### 4c. Frontend

New `pages/settings/backup.rs`, a "Backup & Data" tab (Administration
group, admin-gated — matches Notifications/Security/Users). Export
triggers a same-origin fetch + blob download (no new browser API beyond
what a `<a download>`-less flow needs: fetch, blob, `URL.createObjectURL`,
a synthetic click). Import: a file input, upload the parsed JSON, render
the returned counts. Nix: fetch the plain-text snippet into a `<pre>` with
a Copy button (`navigator.clipboard`).

## 5. Dependencies

None new.

## 6. Files touched

Backend: `db/models.rs`, `api/config_export.rs` (new), `api/mod.rs`,
`api/openapi.rs`.
Frontend: `pages/settings/backup.rs` (new), `pages/settings/mod.rs`.

## 7. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Import silently overwrites/wipes existing data | Additive-only, by design — no delete/replace path exists in this endpoint at all |
| Uploaded file flips auth mode | `settings.auth_mode` is deliberately never applied on import |
| Export leaks a credential (webhook secret, session secret) | `NotificationChannel.secret` excluded from JSON export; `auth.secret`/`webhook_secret` excluded from Nix export |
| Generated Nix implies more than the module supports | Scoped exactly to what `nix/module.nix`'s freeform `settings` already consumes — verified against the actual module before writing this spec, not assumed from the original pitch |
| Duplicate quick links/channels on repeated import | No natural dedup key exists for either; documented as a minor, correctable side effect rather than silently deduping on a heuristic (e.g. title match) that could itself skip a legitimately different entry |

## 8. Approved validation commands

Same as established: `cargo fmt --all -- --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test -p vexboard-server`,
`cargo build --release --bin vexboard-server`, `scripts/preflight.ps1`,
plus (new since the logs feature) `cargo check`/`clippy --target
wasm32-unknown-unknown -p vexboard-frontend`.
