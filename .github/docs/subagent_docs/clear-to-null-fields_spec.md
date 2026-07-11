# BUG-4 — `update_service`/`update_group` Can Never Clear Nullable FK/String Fields — Spec

## Current State Analysis

Four fields across two update handlers use `Option<T>.or(existing)` to merge a partial-update
payload onto the existing row:

- `crates/vexboard-server/src/api/services.rs:393` — `let group_id =
  payload.group_id.or(existing.group_id);`
- `crates/vexboard-server/src/api/services.rs:379` — `let discovery_source =
  payload.discovery_source.or(existing.discovery_source);`
- `crates/vexboard-server/src/api/groups.rs:165-166` — `let icon = payload.icon.or(existing.icon);`
  / `let color = payload.color.or(existing.color);`

All four DTO fields are plain `Option<T>` (`UpdateService.group_id: Option<i64>`,
`UpdateService.discovery_source: Option<String>`, `UpdateGroup.icon: Option<String>`,
`UpdateGroup.color: Option<String>` — `crates/vexboard-server/src/db/models.rs:71-99`). With
serde's default `Option<T>` deserialization, a JSON body with the key explicitly set to `null`
deserializes identically to the key being omitted entirely — both produce `None`. `.or(existing)`
therefore can never distinguish "the client wants to clear this field" from "the client didn't
mention this field," so it always falls back to the existing value. A JSON `{"group_id": null}`
PUT is a silent no-op on that field.

This differs from the already-working convention used a few lines above for `description`,
`url`, and `icon` on services (`crates/vexboard-server/src/api/services.rs:381-392`), which
represents "clear" with an explicit **empty string** sentinel (`v.is_empty()` → `None`) rather
than relying on JSON `null` — that pattern is unaffected by this bug and is left untouched.

The frontend edit-service flow (`crates/vexboard-frontend/src/pages/dashboard/modals.rs`,
`on_edit_save`) already sends `"group_id": data.group_id` where `data.group_id: Option<i64>` —
selecting "No group" in the dropdown (`crates/vexboard-frontend/src/components/modal_edit.rs:165`)
sets `selected_group_id` to `None`, which serializes to JSON `null`. The frontend is already
emitting the semantically-correct wire format; the fix is entirely server-side deserialization
handling. No frontend change is required for `group_id`. `discovery_source`, group `icon`, and
group `color` have no dedicated UI clear affordance today (out of scope to add one — this fix
only makes the API correctly honor an explicit `null` when sent).

## Problem Definition

`Option<T>` cannot distinguish "field omitted" from "field explicitly null" in a partial-update
DTO, so explicit-null requests to clear `group_id`, `discovery_source`, group `icon`, or group
`color` are silently ignored.

## Proposed Solution

Use the standard "double `Option`" pattern (`Option<Option<T>>` via a custom
`deserialize_with`) for these four fields only:

- Key omitted from JSON → outer `None` → keep existing value (unchanged behavior).
- Key present with JSON `null` → `Some(None)` → clear the field to `NULL`.
- Key present with a value → `Some(Some(v))` → set the field to `v`.

A single shared helper function `deserialize_some` (the common name for this pattern; wraps the
inner deserialize result in `Some`) is added once and reused across the four fields, avoiding
duplication and avoiding any new external dependency (`serde_with` is not needed — this is
~6 lines of plain `serde::Deserialize`).

## Implementation Steps

### 1. `crates/vexboard-server/src/db/models.rs` — add helper + change 4 field types

Add near the top of the file (or directly above `UpdateService`):
```rust
/// Distinguishes "field omitted" (`None`) from "field explicitly `null`" (`Some(None)`) in a
/// partial-update JSON body, so a PATCH/PUT payload can request clearing a nullable column
/// instead of that key's absence being silently treated as "keep existing value."
fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}
```

In `UpdateService`, change:
```rust
pub discovery_source: Option<String>,
...
pub group_id: Option<i64>,
```
to:
```rust
#[serde(default, deserialize_with = "deserialize_some")]
#[schema(value_type = Option<String>)]
pub discovery_source: Option<Option<String>>,
...
#[serde(default, deserialize_with = "deserialize_some")]
#[schema(value_type = Option<i64>)]
pub group_id: Option<Option<i64>>,
```

In `UpdateGroup`, change:
```rust
pub icon: Option<String>,
pub color: Option<String>,
```
to:
```rust
#[serde(default, deserialize_with = "deserialize_some")]
#[schema(value_type = Option<String>)]
pub icon: Option<Option<String>>,
#[serde(default, deserialize_with = "deserialize_some")]
#[schema(value_type = Option<String>)]
pub color: Option<Option<String>>,
```
(`#[schema(value_type = ...)]` keeps the generated OpenAPI schema as a plain nullable field
instead of a nested-optional shape utoipa can't represent naturally — utoipa 5, already the
pinned version, supports this attribute.)

### 2. `crates/vexboard-server/src/api/services.rs` — consume the double option

Change:
```rust
let discovery_source = payload.discovery_source.or(existing.discovery_source);
```
to:
```rust
let discovery_source = payload
    .discovery_source
    .unwrap_or(existing.discovery_source);
```
and:
```rust
let group_id = payload.group_id.or(existing.group_id);
```
to:
```rust
let group_id = payload.group_id.unwrap_or(existing.group_id);
```
(`Option::unwrap_or` on the outer `Option<Option<T>>`: `None` outer → keep `existing`; `Some(inner)`
→ use `inner`, which is `None` for explicit clear or `Some(v)` for a new value — exactly the
desired three-way semantics.)

### 3. `crates/vexboard-server/src/api/groups.rs` — consume the double option

Change:
```rust
let icon = payload.icon.or(existing.icon);
let color = payload.color.or(existing.color);
```
to:
```rust
let icon = payload.icon.unwrap_or(existing.icon);
let color = payload.color.unwrap_or(existing.color);
```

No frontend changes are required — `modals.rs`'s existing `on_edit_save` payload for
`group_id` already sends `null` on purpose when "No group" is selected, which is now correctly
interpreted as "clear."

## Dependencies

None new — the double-`Option` pattern is implemented with plain `serde::Deserialize`, no
`serde_with` or other crate needed.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Any existing API caller that omits `group_id`/`discovery_source`/`icon`/`color`
  entirely from a partial-update body must continue to see "keep existing" — verified by the
  `#[serde(default)]` attribute, which makes the field default to the outer `None` when the key
  is absent (not required, no error).
- **Risk:** utoipa schema generation for `Option<Option<T>>` without the `value_type` override
  could produce a malformed/misleading OpenAPI spec.
  **Mitigation:** `#[schema(value_type = Option<T>)]` on each affected field keeps the
  generated schema representing the field as a plain nullable value, matching actual wire
  behavior (the outer optionality is a Rust-side deserialization concern, not a wire-format
  concern — omission vs. `null` is standard JSON/OpenAPI `nullable` semantics already).

## Test Plan

`cargo test -p vexboard-server` — no existing test exercises `update_service` or `update_group`
directly (no test route coverage for PUT /services/{id} or /groups/{id} in the current suite),
so behavior is unaffected for the currently-tested paths. No new test is added: this project's
test harness (`crate::tests`) has no precedent for JSON-body partial-update assertions, and
adding that harness is disproportionate to a targeted deserialization fix; the change is
mechanically verified by `cargo build`/`clippy` type-checking the new `Option<Option<T>>` flow
end-to-end (DTO → deserialize_with → `unwrap_or` → bind) compiles and matches existing bind
types (`sqlx::query(...).bind(group_id)` already binds `Option<i64>`, unaffected by the DTO
type change since `group_id`'s *local variable* type after `unwrap_or` remains `Option<i64>`).
