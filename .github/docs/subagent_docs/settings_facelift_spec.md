# Settings Page Facelift — Specification

Status: **Phase 1 complete — awaiting approval before Phase 2.**
Visual rendering: https://claude.ai/code/artifact/f806b256-4b7f-43f7-a519-c3ec7119c68b

## 1. Current State Analysis

- `crates/vexboard-frontend/src/pages/settings.rs` — 503 lines rendering **five** controls
  (theme toggle, sidebar mode, a static discovery blurb, auth mode, user management).
- `crates/vexboard-server/src/config.rs` — ~30 configurable options across 8 structs
  (`server`, `database`, `auth`, `discovery`, `docker`, `probe`, `metrics`, `notifications`).
  All but `auth.mode` are file/env-only and unreachable from the UI.
- `crates/vexboard-server/src/api/settings.rs` — 125 lines exposing exactly one key
  (`/auth-mode`), backed by the generic `db::get_setting` / `db::set_setting` key-value table.
- Layout: `.settings-list` / `.settings-row` in `style/main.css:495-620` — flat list, fixed
  240px label column vs. fluid control column.

### Defects

1. **Coverage** — 5 of ~30 settings exposed.
2. **Structure** — one flat scroll; no grouping or hierarchy. Theme sits three rows from
   "disable authentication".
3. **Proportion** — a lone button and a full user table occupy the same column span.
4. **Save model** — mixed instant-API / localStorage / restart-required with no signalling.
5. **Error handling** — every `spawn_local` discards its `Result`; failures are silent.
6. **Styling** — user management built from inline `style=` strings and `onmouseover`
   attributes, bypassing the theme tokens.

## 2. Proposed Solution

Two-pane layout: a searchable section rail (236px) + a content pane with a fixed 600px
reading measure. Nine sections in three groups:

| Group | Sections |
|---|---|
| Interface | Appearance, Dashboard |
| Services | Monitoring, Discovery, Notifications |
| Administration | Security, Users, Backup & Data, About |

Within a pane: bordered cards as sub-groups; rows of label + help text (left) / control
(right) separated by hairlines; a sticky footer save bar showing the dirty count with
Save / Discard. State chips `Restart needed` and `config.toml` mark settings the running
process cannot reload and values pinned outside the database.

Section selection reflects into the URL (`/settings/security`).

## 3. Implementation Steps

1. **Primitives** — `SettingsRow`, `Toggle`, `Segmented`, `NumberField`, `TagInput`
   components + CSS appended to `style/main.css`.
   *Verify:* components render in isolation; no inline `style=` remains in settings code.
2. **Shell** — `pages/settings/mod.rs` (rail + pane) with one module per section; add a
   nested route so `/settings/:section` resolves.
   *Verify:* all nine sections navigate; back button works; deep link loads correct pane.
3. **Migrate existing five controls** unchanged into their panes.
   *Verify:* theme, sidebar, auth-mode and user CRUD behave exactly as before.
   **This is a shippable milestone with zero backend change.**
4. **Generalise the settings API** — widen `api/settings.rs` to
   `GET /api/v1/settings` and `PATCH /api/v1/settings`, returning each key with
   `{ value, source: default|file|db, restart_required }` over the existing key-value table.
   Keep `/auth-mode` as a compatibility shim.
   *Verify:* `cargo test -p vexboard-server` covers read, write, source resolution,
   admin-gating and audit-log emission.
5. **Config layering** — insert the DB layer between `/etc/vexboard/config.toml` and env
   vars in `AppConfig::load`; hot-reload the values that are safe to change at runtime
   (probe intervals, discovery filters, notification rules) via a watch channel.
   *Verify:* a PATCHed probe interval takes effect without restart; env vars still win.
6. **Remaining panes** land with their backing features (see feature roadmap).
7. **Error/success states** — every save resolves to a toast or inline error.
   *Verify:* forced 500 from the API produces a visible message.

## 4. Dependencies

None new. Leptos 0.8, existing `gloo-net`, existing `db::{get,set}_setting`.
Context7 lookup not required — no new external library.

## 5. Configuration Changes

`AppConfig::load` gains a database source layer. Precedence becomes:
env (`VEXBOARD_*`) > `/etc/vexboard/config.toml` > database > `config/default.toml`.
File-pinned values are reported with `source: "file"` and rendered read-only, so NixOS
declarative config is never silently overwritten by the UI.

## 6. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| UI writes conflict with declarative Nix config | `source` field marks file-pinned keys read-only in the UI |
| Hot-reload introduces races in probe/discovery loops | Restrict reload to loops that already re-read config each tick; everything else keeps the restart chip |
| Route change breaks existing `/settings` links | `/settings` redirects to `/settings/appearance` |
| Scope creep from 5 → 30 controls in one PR | Step 3 is a shippable stopping point; steps 4+ are separate PRs |

## 7. Approved Validation Commands

`cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`,
`cargo test -p vexboard-server`, `cargo build --release --bin vexboard-server`,
`scripts/preflight.ps1`. Frontend WASM build only if Trunk + `wasm32-unknown-unknown`
are confirmed present.
