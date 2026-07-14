# Dashboard Blink/Scroll-Jump + Admin Role Regression — Specification

## Investigation Note

The two prior blink fixes (94b1078, 5513366) and the first attempt in this
session (`held_history`) all failed because they were reasoned from source
without observing the running system. Root cause was only found by reading the
Leptos 0.8.19 source directly.

## Problem 1 — Dashboard blink + forced scroll-to-top

### Root cause (confirmed at source level)

`leptos-0.8.19/src/transition.rs:20-25` documents `Transition` as:

> "If **any** Resource is read in the `children` of this component, it will show
> the `fallback` while they are loading. ... Unlike **`Suspense`**, this will not
> fall back to the `fallback` state **if there are further changes after the
> initial load**."

This is an explicit statement that `Suspense` **does** revert to its fallback on
every post-initial-load resource change.

Each `ServiceCard` creates a `history: LocalResource` and reads it inside
`ServiceGrid`'s `<Suspense>` boundary. Every SSE `probe` tick updates
`live_status`, which re-runs those history resources. Any one of them going
pending re-suspends the **entire boundary**, which swaps the whole grid for the
skeleton fallback and then swaps it back:

- cards "blink out then back in" — the fallback replacing real content;
- the grid collapses to 3 short skeleton cards, so the scroll container's height
  drops and the browser clamps scroll position to the top.

`GroupSection` and `QuickLinksSection` use `fallback=|| ()`, so in Group mode the
section blanked entirely.

This explains why the prior fixes failed: they changed *which* resources refetch,
but **any** pending resource under the boundary re-suspends it.

### Solution

Replace `<Suspense>` with `<Transition>` in all three dashboard boundaries.
`Transition` keeps resolved content on screen while resources reload underneath —
the "refresh in the background" behaviour of Homepage / Uptime Kuma.

Retain the `held_history` signal in `service_card.rs`: with `Transition` the grid
no longer tears down, but an individual card's `history.get()` still returns
`None` mid-refetch, which would blank that card's sparkline and jitter its height.
Holding the last resolved value keeps each strip stable.

## Problem 2 — Admin silently demoted to viewer

### Root cause (confirmed against the live database)

`/api/v1/auth/me` and `require_admin` both read the role **exclusively from the
session**, and the role is written into the session **only at login**. A session
carrying a `username` but no `role` therefore resolves to `"viewer"`, and every
admin route 403s — while the `users` table still says `admin`.

Sessions persist in the `tower_sessions` table across upgrades, so any session
created before the roles feature (5e1a1c2) — or whose role write failed — pins a
real admin to viewer permanently.

Symptoms match exactly: still logged in (username present, no redirect), reads
work (`require_auth` only), but the Add button disappears (frontend `is_admin()`
false) and all writes 403.

### Solution

Make the `users` row the source of truth. Add
`middleware::auth::resolve_role(state, session, username)`, which reads the role
from the database and falls back to the session-cached role only for PAM users
(who have no `users` row). Use it in both `/me` and `require_admin`.

This is self-healing for stale sessions and makes role changes take effect
immediately rather than at next login. Requires threading `AppState` into the
`require_admin` layer (`from_fn` → `from_fn_with_state`).

## Implementation Steps

1. `service_grid.rs`, `group_section.rs`, `quick_links_section.rs`:
   `<Suspense>` → `<Transition>`.
2. `middleware/auth.rs`: add `resolve_role`; `require_admin` takes `State<AppState>`.
3. `api/mod.rs`: `router(auth_mode, state)`, use `from_fn_with_state`.
4. `main.rs`: pass `state.clone()` to `api::router`.
5. `api/auth.rs`: `me` uses `resolve_role`.
6. `tests.rs`: regression tests for DB-authoritative role.

## Risks & Mitigations

- **Per-request DB read for role:** one indexed lookup by username on admin
  routes and `/me`. Negligible against SQLite, and correctness here outweighs it.
- **PAM mode:** PAM users have no `users` row; `resolve_role` falls back to the
  session role, preserving existing behaviour.
- **Pre-existing PAM bug (NOT fixed — out of scope):** in `login_pam`, the
  bootstrap admin grant uses `try_claim_setting`, which returns `false` once the
  key exists — *even for the same user*. The bootstrap admin is therefore demoted
  to viewer on their **second** login. Only affects builds with the `pam-auth`
  feature. Reported to the user; not changed here.
