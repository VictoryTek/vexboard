# Sort Mode Persistence — Spec

## Current State Analysis

- Sort toggle UI lives in `crates/vexboard-frontend/src/pages/dashboard/mod.rs`.
- `SortMode` enum defined at lines 16-21 (`AZ`, `Source`, `Group`), no `Default` impl.
- State signal at line 117: `let (sort_mode, set_sort_mode) = signal(SortMode::AZ);` — always hardcoded to `AZ` on component creation, never read from any persistent source.
- Toggle buttons at lines 155-176; on-click handler at line 172: `on:click=move |_| set_sort_mode.set(mode)` — sets the signal in-memory only, never persisted.
- `sort_mode` is passed into `ServiceGrid` (lines 332, 344) and consumed in `crates/vexboard-frontend/src/pages/dashboard/service_grid.rs`.

## Problem Definition

Selecting "Source" or "Group" in the sort toggle is not persisted. On page refresh, `DashboardPage` remounts and re-initializes `sort_mode` to `SortMode::AZ`, discarding the user's prior selection.

## Existing Prior Art (directly reusable pattern)

`crates/vexboard-frontend/src/components/sidebar.rs` lines 12-44 implements the exact pattern needed for a per-browser UI preference, using `web_sys::window().local_storage()`:

```rust
#[cfg(target_arch = "wasm32")]
pub fn load_sidebar_mode_from_storage() -> SidebarMode { ... }

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn load_sidebar_mode_from_storage() -> SidebarMode { SidebarMode::HoverExpand }

#[cfg(target_arch = "wasm32")]
pub fn save_sidebar_mode_to_storage(mode: &SidebarMode) { ... }

#[cfg(not(target_arch = "wasm32"))]
pub fn save_sidebar_mode_to_storage(_mode: &SidebarMode) {}
```

Loaded once before signal creation in `main.rs` (around line 32-37), saved explicitly at the call site whenever the mode changes.

There is no per-user backend preferences table — only a global admin-gated `settings` KV table (`crates/vexboard-server/src/db/migrations/001_init.sql` lines 45-48) used for server-wide config like `auth_mode`. That table is not an appropriate fit for a personal, non-admin, per-browser UI toggle. **No backend/dependency changes needed** — this is a pure frontend, client-side `localStorage` fix, consistent with how `SidebarMode` is already handled. No new external dependency is introduced (uses existing `web_sys`, already a dependency), so Context7 verification is not required per the Dependency Policy ("Internal code changes with no new dependencies").

## Proposed Solution

Mirror the sidebar pattern exactly, scoped locally to `crates/vexboard-frontend/src/pages/dashboard/mod.rs` (no need for a separate module or context, since `sort_mode` is local component state, not shared via context like `SidebarMode`):

1. Add two free functions near the `SortMode` enum:
   - `load_sort_mode_from_storage() -> SortMode` (cfg-gated wasm32 / non-wasm32 like the sidebar functions), reading `localStorage` key `"vexboard_sort_mode"`, mapping `"source"` → `Source`, `"group"` → `Group`, anything else → `AZ`.
   - `save_sort_mode_to_storage(mode: &SortMode)` (cfg-gated), writing the same key with values `"az"`, `"source"`, `"group"`.
2. Replace line 117's initializer: `let (sort_mode, set_sort_mode) = signal(load_sort_mode_from_storage());`
3. Update the on-click handler at line 172 to also persist:
   ```rust
   on:click=move |_| {
       set_sort_mode.set(mode);
       save_sort_mode_to_storage(&mode);
   }
   ```

## Implementation Steps

1. In `crates/vexboard-frontend/src/pages/dashboard/mod.rs`, add `load_sort_mode_from_storage` / `save_sort_mode_to_storage` functions directly below the `SortMode` enum (after line 21), following the exact cfg-gating pattern from `sidebar.rs`.
2. Change line 117 to call `load_sort_mode_from_storage()` instead of hardcoding `SortMode::AZ`.
3. Change the `on:click` handler at line 172 to call `save_sort_mode_to_storage(&mode)` after `set_sort_mode.set(mode)`.
4. No other files require changes — `ServiceGrid` already consumes `sort_mode` reactively via the existing prop.

## Dependencies

None new. Uses existing `web_sys` (already a workspace dependency, confirmed in use by `sidebar.rs`).

## Configuration Changes

None. This is purely client-side `localStorage`; no `config/default.toml` changes.

## Risks and Mitigations

- **Risk:** `web_sys::window().local_storage()` can return `Err`/`None` in restrictive environments (e.g., privacy mode). **Mitigation:** matches the existing sidebar pattern's use of `.ok().flatten()` chains with `.unwrap_or_default()`/no-op fallback — fails silently to default `AZ` behavior, no panics.
- **Risk:** Non-wasm32 (native test/build) target must still compile. **Mitigation:** cfg-gated stub functions identical in shape to the sidebar ones ensure native builds compile untouched.
- **Risk:** Since `SortMode` derives `PartialEq` but not `Default`, keep using explicit `AZ` as the fallback value in the loader rather than adding a `Default` impl (avoids unrelated trait surface change; matches Surgical Changes principle).
