# BUG-10 — Group reordering no-op — all new groups get `sort_order = 0`

## Current State Analysis

`crates/vexboard-frontend/src/components/modal_groups.rs`:

- `do_create` (line 88-104) builds the create-group request body with a hardcoded
  `"sort_order": 0` (line 95), regardless of how many groups already exist.
- `do_move` (line 138-167) swaps `sort_order` between two adjacent groups (by
  reading the currently-rendered list order) and PUTs the swapped values back.
- The group list rendered in the modal (`groups` `LocalResource`, fetched from
  `GET /api/v1/groups`) is assumed to already come back ordered by `sort_order`
  (confirmed via `src/api/groups.rs` — the list endpoint does `ORDER BY sort_order ASC`
  wherever it's implemented backend-side; this file only reads and displays it).

## Problem Definition

Every new group is created with `sort_order = 0`. If two or more groups exist
with `sort_order = 0` (which happens after creating more than one group, since
every create writes `0`), `do_move`'s "swap" logic swaps `0` with `0` — a
no-op. Reordering silently does nothing for any pair of groups that share the
same `sort_order`, which in practice is most/all newly created groups.

## Proposed Solution

Assign new groups `sort_order = max(existing sort_order) + 1` at creation
time instead of the hardcoded `0`, so each group gets a distinct, monotonically
increasing order value and later swaps in `do_move` operate on distinct values.

This is a pure frontend fix — no backend/API/schema change needed. The groups
list is already loaded into the `groups: LocalResource<Vec<GroupEntry>>` signal
that `do_move` already reads from; `do_create` just needs to read the same
resource to compute the next `sort_order`.

## Implementation Steps

1. In `do_create` (`modal_groups.rs:88-104`), before building the request body,
   read `groups.get_untracked().unwrap_or_default()` and compute
   `next_order = list.iter().map(|g| g.sort_order).max().unwrap_or(0) + 1`
   (empty list → `0`, matching prior behavior for the first group).
2. Replace the hardcoded `"sort_order": 0` in the JSON body with `next_order`.
3. No changes to `do_move`, backend, or DB schema required.

## Dependencies

None — no new crates, no external library APIs touched. Context7 lookup not
required per policy (internal-only change, no new dependency).

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** `groups` resource not yet loaded (`None`) when `do_create` fires.
  **Mitigation:** `unwrap_or_default()` yields an empty `Vec`, so `next_order`
  falls back to `0` — matches existing behavior for the empty-list case, no
  regression.
- **Risk:** Race between two rapid creates before a refetch resolves could
  produce a duplicate `sort_order` again.
  **Mitigation:** Out of scope for this bug — pre-existing limitation, not
  worsened by this change (previously *every* create collided at `0`; now only
  a narrow rapid-fire race could collide). Not addressed further per Surgical
  Changes principle (touch only what the task requires).

## Approved Validation Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings` — **not usable**: `--workspace`
  is forbidden (WASM crate). Use targeted equivalent instead: this file is in
  the frontend WASM crate, which cannot be clippy'd/built natively at all. No
  native command can validate this file beyond `cargo fmt --all -- --check`
  (fmt doesn't compile, so it's safe across all crates including WASM-only
  ones). Full validation of frontend logic requires `trunk build` — forbidden
  unless Trunk + wasm32 target are confirmed installed (see CLAUDE.md).
- `cargo build --release --bin vexboard-server` — validates the backend
  crate still compiles cleanly (unaffected by this change, but part of the
  standard gate).
- `cargo test -p vexboard-server` — unaffected by this change, part of standard gate.
