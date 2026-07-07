# Phase 1 Spec — remote_badge

**Date:** 2026-07-07

## Current State Analysis

Service origin badges are rendered from the `discovery_source` string field
(`Service.discovery_source: Option<String>`,
`crates/vexboard-server/src/db/models.rs:18`), populated only by the discovery
subsystems:
- `crates/vexboard-server/src/discovery/systemd.rs:146` sets `"systemd"`.
- `crates/vexboard-server/src/discovery/docker.rs:57-61,235` sets `"docker"` or
  `"podman"` (this covers both local **and** remote Docker/Podman hosts
  configured via `config.docker.sockets` with a `tcp://` URL — those are true
  discovery-backed rows and are already correctly badged today).

The frontend renders the badge in
`crates/vexboard-frontend/src/components/service_card.rs:53-69`
(`source_badge`), matching on the lowercased `discovery_source`:
`"docker"` → Docker/`#0db7ed`, `"podman"` → Podman/`#892ca0`,
`"systemd"` → Systemd/`#e8873a`. If `discovery_source` doesn't match any of
these, it falls back to checking whether `systemd_unit` ends in `.service`
(covers legacy/manually-entered systemd units); otherwise the badge is
`None` and nothing renders.

A second, independent implementation exists in
`crates/vexboard-frontend/src/components/discovery_panel.rs:38-52`
(`DiscoveredUnitFe::source_label`/`source_color`) for the "discovered, not yet
added" panel — every entry there always has a real `source` from active
discovery, so it is out of scope for this change (nothing to badge as
"Remote" there).

## Problem Definition

Services added manually by the user (typically pointing at a URL on another
machine) have no `discovery_source` and no `systemd_unit`, because VexBoard's
discovery only runs against systemd/Docker/Podman on hosts it's explicitly
configured to reach — it cannot inspect an arbitrary manually-entered URL to
determine whether the thing behind it is systemd- or container-managed. This
is a genuine, permanent data gap, not a bug: that information was never
collected. Today these services silently render with no origin badge at all,
which reads as inconsistent/incomplete next to Docker/Podman/Systemd cards.

## Proposed Solution

Add a fourth badge variant, "Remote", used whenever a service has no
recognized `discovery_source` and no systemd-style `systemd_unit`. This
communicates "this service isn't locally discovered/managed by VexBoard" —
which is the accurate framing — rather than leaving the badge slot empty.

No backend change needed: no new field, no DB migration, no API change. This
is presentation-only — the "Remote" label is derived client-side from the
*absence* of a discovery source, mirroring the existing fallback logic
already in `service_card.rs`.

Badge color: `#5b8def` (a blue distinct from the existing Docker blue
`#0db7ed`, Podman purple `#892ca0`, and Systemd orange `#e8873a`) — chosen for
sufficient contrast against the three existing badge hues.

### Logic change in `service_card.rs`

Replace the final `None` fallback arm of `source_badge` with
`Some(("Remote".to_string(), "#5b8def".to_string()))`, so every service now
always shows a badge:
1. `discovery_source` matches docker/podman/systemd → existing badge.
2. Else if `systemd_unit` ends in `.service` → Systemd badge (unchanged).
3. Else → **Remote** badge (new; previously `None`).

## Affected Files

1. `crates/vexboard-frontend/src/components/service_card.rs` — badge match
   logic (lines ~53-69).

## Implementation Steps

1. In `source_badge`'s construction, change the trailing `else { None }` arm
   to return the new Remote badge tuple instead of `None`.
2. No other files require changes — `discovery_panel.rs` is unaffected (all
   entries there have a real discovery source), and no backend/API/DB touch
   is needed since this is purely a frontend rendering fallback.

## Dependencies

None — no new external library usage, internal-only change. Context7 lookup
not required per policy (no new dependency, no new external API).

## Configuration Changes

None.

## Build/Test Commands (Phase 3)

Frontend crate is WASM-only and cannot be compiled/tested for the native
target (see CLAUDE.md Resource Constraints) — `trunk build`/`trunk serve`
require confirming Trunk + `wasm32-unknown-unknown` are installed, which is
unconfirmed in this environment. Applicable safe checks:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings` (workspace clippy is scoped per
  crate by clippy itself and does not attempt native linking the way `cargo
  build --workspace` would; confirm this still succeeds for the frontend
  crate's clippy pass, or fall back to formatting-only verification if not)

No FORBIDDEN COMMANDS used.

## Risks and Mitigations

- **Risk:** "Remote" badge could be misleading for a manually-added service
  that actually points at something running on the very same host VexBoard
  runs on (not truly "remote").
  **Mitigation:** Accepted — VexBoard has no way to distinguish these cases
  since it has no signal at all for manually-entered services; "Remote" is
  the closest accurate label for "not discovered/managed by VexBoard", which
  is what the user explicitly requested.
- **Risk:** Duplicated match logic between `service_card.rs` and
  `discovery_panel.rs` could drift.
  **Mitigation:** Out of scope — `discovery_panel.rs` doesn't need a Remote
  case (pre-existing duplication, not introduced by this change; Surgical
  Changes principle applies).
