# Claimed Docker/Podman Containers Store a Fake systemd_unit — Spec (BUG-3)

Source: MASTER_PLAN.md HIGH PRIORITY / Data Loss / Functional Breakage / BUG-3 (B-H5)

## Current State Analysis — important correction to the master-plan description

The master plan describes the symptom as: claiming a Docker container posts
`systemd_unit: <container-name>`, the probe scheduler gives `systemd_unit` priority
over `url`, so the container is probed via `unit_active_state()` against a
nonexistent unit and shows a permanent "down" status.

**That priority bug no longer exists in the current codebase.** It was already
fixed by commit `d5d1f399` ("fix: remote service probe", pre-dates this session) in
both call sites the master plan cites:

- `crates/vexboard-server/src/probe/mod.rs:38-46` — the scheduled probe loop now
  computes `use_systemd = svc.systemd_unit.is_some() && !matches!(svc.discovery_source.as_deref(), Some("docker") | Some("podman"))`
  before choosing between `probe_systemd_unit` and `probe_service` (URL-based).
- `crates/vexboard-server/src/api/services.rs` (`create_service`'s immediate
  post-create probe) — identical guard added in the same commit.

Verified by reading both files directly and via `git show d5d1f399`. So a claimed
Docker/Podman container is already probed via its URL (when present), never via a
fake D-Bus unit lookup, and does not show a permanent false "down".

**What's still actually broken:** the *third* file the master plan cites,
`crates/vexboard-frontend/src/components/discovery_panel.rs:98-99`, still
unconditionally sends `"systemd_unit": unit_name` in the claim payload regardless of
`discovery_source`:
```rust
let body = serde_json::json!({
    "systemd_unit": unit_name,
    "discovery_source": source,
    ...
});
```
So every claimed Docker/Podman container still gets a `systemd_unit` column value
in the database equal to its container name — which is not a real systemd unit.
Today this is inert (the backend guards above ignore it for non-systemd sources),
but it's misleading stored data and a latent trap: any future code that checks
"does this service have a `systemd_unit`" without also checking
`discovery_source` (e.g. a future uniqueness constraint per BUG-8, or any new UI
badge) would silently reintroduce exactly the class of bug this master-plan entry
describes.

## Problem Definition

The "set" side of the systemd_unit/discovery_source relationship was never fixed to
match the "use" side: the discovery panel writes a fake `systemd_unit` value for
non-systemd claims, even though nothing should ever read it as such.

## Proposed Solution

Only include a real `systemd_unit` value when the claimed unit's source is
`"systemd"`; send `null` for `"docker"`/`"podman"` claims, matching the guard
already used on the read side.

```rust
let on_save = Callback::new(move |data: EditFormData| {
    let source = selected_source.get_untracked();
    let unit_name = selected_unit_name.get_untracked();
    let systemd_unit = if source.as_deref() == Some("systemd") {
        unit_name.clone()
    } else {
        None
    };
    spawn_local(async move {
        let body = serde_json::json!({
            "systemd_unit": systemd_unit,
            "discovery_source": source,
            ...
        });
        ...
    });
});
```

## Implementation Steps

1. `crates/vexboard-frontend/src/components/discovery_panel.rs:94-108` — compute a
   `systemd_unit` local that's `None` unless `source == Some("systemd")`; use it in
   place of the raw `unit_name` in the JSON body.

## Dependencies

None.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** None identified — this only stops writing a value that nothing should
  read as a real systemd unit anyway (the backend already ignores it for
  docker/podman sources).
- Per CLAUDE.md constraints, validation is `cargo fmt --all -- --check` and
  `cargo clippy --workspace -- -D warnings` (both natively type-check
  `vexboard-frontend`); no `trunk build` (FORBIDDEN COMMANDS).

## Files

- `crates/vexboard-frontend/src/components/discovery_panel.rs:94-108`
- (No backend changes — `probe/mod.rs` and `api/services.rs` already correct as of
  commit `d5d1f399`.)
