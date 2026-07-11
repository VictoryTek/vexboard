# SEC-7 — Audit Log Exposed to Viewer Role — Spec

## Current State Analysis

`crates/vexboard-server/src/api/mod.rs:32-46` builds `viewer_protected`, a router gated only by
`require_auth` (any authenticated session, viewer or admin), and nests `audit::router()`
(`/api/v1/audit`) inside it alongside genuinely read-only, low-sensitivity resources (services,
groups, quick-links, metrics).

`audit::list_audit` (`crates/vexboard-server/src/api/audit.rs:45-93`) returns paginated
`audit_log` rows including `actor`, `action`, `detail` (which contains, per
`crates/vexboard-server/src/api/auth.rs:152,178`, the attempted username on login failures),
and `ip_addr`. A viewer-role user can therefore enumerate every user account referenced in the
system (via login-failure/success actor and detail fields), watch every admin action
(user creation/deletion/role changes, service/group mutations), and see client IP addresses —
none of which a read-only dashboard viewer needs or should have.

`admin_protected` (`crates/vexboard-server/src/api/mod.rs:49-64`) is gated by `require_admin`
and already hosts comparable sensitive/mutating resources (users, settings, discovery).

## Problem Definition

The audit log is a security-sensitive, admin-only resource that is currently reachable by any
authenticated viewer.

## Proposed Solution

Move the `/api/v1/audit` nest from `viewer_protected` to `admin_protected` in
`crates/vexboard-server/src/api/mod.rs`. No change needed inside `api/audit.rs` — it already
exposes a single `router()` (not split into read/admin variants), which is the correct shape
once it's nested under the admin-only router.

## Implementation Steps

1. In `crates/vexboard-server/src/api/mod.rs`, remove `.nest("/api/v1/audit",
   audit::router())` from the `viewer_protected` router construction (lines 32-41).
2. Add `.nest("/api/v1/audit", audit::router())` to the `admin_protected` router construction
   (lines 49-59).

No other files require changes — `audit::router()`'s shape is unaffected; only which
middleware layer it sits under changes.

## Dependencies

None.

## Configuration Changes

None. Frontend impact: any frontend code fetching `/api/v1/audit` as a non-admin viewer will
now receive 403 instead of 200 — per the MASTER_PLAN this is the intended fix, and FEAT-5
(Audit log viewer page, not yet implemented) is the follow-up that will add an admin-gated UI
surface for this endpoint. No existing frontend page currently calls `/api/v1/audit` (per
FEAT-5's description: "The frontend never calls it"), so there is no frontend regression risk.

## Risks and Mitigations

- **Risk:** None identified — the endpoint currently has no frontend caller, so restricting
  it to admins has no functional impact on the existing UI, only removes viewer-role API access.

## Test Plan

`cargo test -p vexboard-server` — no existing test exercises `/api/v1/audit` directly. Given
the change is a one-line router-nesting move (verified by the existing
`test_admin_route_as_viewer_returns_403` test, which already validates the `require_admin`
middleware layer works correctly for other admin-only routes using the identical mechanism),
no new test is added; the fix reuses an already-proven middleware guard on an existing router
composition, not new logic.
