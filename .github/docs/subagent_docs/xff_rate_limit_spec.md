# X-Forwarded-For Trust & Rate Limiter Hardening — Spec (SEC-2)

Source: MASTER_PLAN.md HIGH PRIORITY / Security / SEC-2 (B-H1, A-A7)

## Current State Analysis

- `client_ip()` (`crates/vexboard-server/src/api/auth.rs:25-36`) unconditionally
  prefers the **first** entry of a client-supplied `X-Forwarded-For` header over the
  real socket address from `ConnectInfo<SocketAddr>`. Any direct client (no proxy
  involved at all) can set this header to an arbitrary value and it will be trusted.
- This `client_ip()` result feeds two things in `login()` (`api/auth.rs:70` onward):
  1. `state.login_limiter.check(ip)` — the per-IP login attempt budget
     (`rate_limit.rs`).
  2. The audit log (`db::audit::insert(..., Some(ip_str))`) for both
     `auth.login_success` and `auth.login_failure` events.
- Because the header is fully attacker-controlled, an attacker can send a fresh
  random IP on every login attempt, giving each attempt its own bucket in
  `LoginRateLimiter`'s `HashMap<IpAddr, VecDeque<Instant>>` — completely defeating
  the rate limit — while also polluting the audit log with fabricated IPs (any
  investigator reading login-failure audit entries would see attacker-chosen noise
  instead of the real source).
- `LoginRateLimiter::check()` (`rate_limit.rs:25-38`) evicts expired timestamps from
  an IP's `VecDeque` but never removes the `HashMap` entry once its deque is empty.
  Combined with the spoofing above, the map grows by one entry per spoofed IP,
  forever (unbounded memory growth).
- There is no existing `auth.behind_proxy` (or similar) config flag — every other
  boolean in `AuthConfig` (`config.rs:41-66`) already follows a `#[serde(default)]`
  pattern for backwards-compatible new options.

## Problem Definition

1. `client_ip()` trusts a client-controlled header by default, in a deployment
   context (self-hosted dashboard) where most instances are *not* behind a reverse
   proxy at all — the header should be ignored unless the operator explicitly says
   there is a trusted proxy in front.
2. Even for deployments that *are* behind a proxy, taking the first (leftmost) XFF
   entry is wrong — that's the original client-supplied value, which a client can
   still prepend to before the proxy appends the real one. The trustworthy value in
   a standard single-hop reverse-proxy setup is the **last** (rightmost) entry, which
   the proxy itself appends/overwrites.
3. The rate limiter's per-IP map never shrinks.

## Proposed Solution

### 1. `auth.behind_proxy` config flag

Add to `AuthConfig` (`config.rs`):
```rust
/// Set to true when the server sits behind a reverse proxy that sets
/// X-Forwarded-For. When false (default), the header is ignored entirely and
/// the real socket address is always used — safe for direct-exposure deployments,
/// where the header would otherwise be fully attacker-controlled.
#[serde(default)]
pub behind_proxy: bool,
```
Defaults to `false` (safe default: header ignored). No entry needed in
`config/default.toml` unless the project convention is to always list every key
explicitly there — match whatever the file already does for `secure_cookies`
(currently omitted from `default.toml`, relying on `#[serde(default)]`), so no
`default.toml` change needed.

### 2. Only honor XFF, and only the last hop, when enabled

Change `client_ip()`'s signature to take the flag and change which entry is used:
```rust
fn client_ip(connect_info: &ConnectInfo<SocketAddr>, headers: &HeaderMap, behind_proxy: bool) -> IpAddr {
    if behind_proxy {
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next())
            .and_then(|s| s.trim().parse::<IpAddr>().ok())
        {
            return forwarded;
        }
    }
    connect_info.0.ip()
}
```
`rsplit(',').next()` yields the last comma-separated segment (the rightmost/last-hop
entry), replacing the current `split(',').next()` (leftmost/client-supplied entry).
The call site in `login()` passes `state.config.auth.behind_proxy`.

### 3. Evict empty rate-limiter entries

In `LoginRateLimiter::check()` (`rate_limit.rs`), after popping expired timestamps,
remove the map entry if its deque is now empty (before deciding whether to insert a
fresh attempt), so IPs with no attempts in the current window don't accumulate empty
`VecDeque`s forever:
```rust
pub fn check(&self, ip: IpAddr) -> bool {
    let now = Instant::now();
    let cutoff = now - self.window;
    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    let attempts = state.entry(ip).or_default();
    while attempts.front().is_some_and(|t| *t < cutoff) {
        attempts.pop_front();
    }
    let allowed = (attempts.len() as u32) < self.max_attempts;
    if allowed {
        attempts.push_back(now);
    }
    if attempts.is_empty() {
        state.remove(&ip);
    }
    allowed
}
```
Note the reordering versus the current code: the emptiness check must happen after
the (possible) push, using the entry still in scope, then removed from the map only
if it ended up empty (i.e., rate-limited calls with an already-empty deque, or the
rare case immediately after eviction with no new push). Since a successful `check()`
always pushes one timestamp, the deque is only empty (and thus prunable) when the
call was rate-limited (`allowed == false`) and the deque had nothing left after
eviction — which is itself a rare edge case (attempts existed prior but all expired
en route to a rate-limited decision, or `max_attempts == 0` disabling the limiter
entirely, in which case `check()` isn't even called per `login()`'s
`login_rate_limit_attempts > 0` guard). Implementation will use the `Entry` API
directly to avoid a double lookup; exact code finalized during implementation.

## Implementation Steps

1. `crates/vexboard-server/src/config.rs` — add `AuthConfig::behind_proxy`
   (`#[serde(default)]`, `bool`).
2. `crates/vexboard-server/src/api/auth.rs` — `client_ip()` takes `behind_proxy: bool`;
   switch to `rsplit` (last hop) instead of `split` (first hop) when enabled; ignore
   the header entirely when disabled. Update the call site in `login()`.
3. `crates/vexboard-server/src/rate_limit.rs` — prune empty `VecDeque` entries from
   the map in `check()`.
4. `crates/vexboard-server/src/tests.rs` — no existing test exercises `client_ip()` or
   the rate limiter directly; existing login tests use `login_rate_limit_attempts: 0`
   (limiter disabled) so they're unaffected. No new test infra changes required
   unless a targeted unit test for `client_ip()`/rate-limit pruning is added (see
   Risks).

## Dependencies

None — no new crate, no external API surface change. `HeaderMap`, `ConnectInfo`, and
`HashMap`/`VecDeque` are already in use; Context7 lookup not applicable (internal
logic change only, per CLAUDE.md's Context7 exemption for "internal code changes with
no new dependencies").

## Configuration Changes

- New `auth.behind_proxy` (default `false`). Operators running VexBoard behind
  nginx/Traefik/Caddy etc. must set this to `true` for `X-Forwarded-For` to be
  honored for rate-limiting/audit purposes; otherwise the real socket peer (likely
  the proxy's own IP) is used for all clients, which is still safe (just less
  granular) — not a regression from today's *intended* behavior, only from the
  currently-broken trust-everything behavior.

## Risks and Mitigations

- **Risk:** `rsplit(',').next()` trusts the last hop assuming a single reverse proxy
  that appends (rather than blindly forwards) the client's real IP. If an operator
  chains multiple proxies without each one appending correctly, the last entry could
  still be attacker-influenced. **Mitigation:** Documented in the config comment as a
  single-hop assumption; matches the master plan's explicit fix guidance ("last hop,
  not first") and is the standard minimal-viable trust model for this class of
  self-hosted app. A configurable "trusted hop count" is out of scope (no such
  request in the master plan entry).
- **Risk:** Flipping `behind_proxy` default risks silently changing rate-limit
  behavior for existing reverse-proxy deployments that relied on the (buggy) old
  behavior. **Mitigation:** Default is `false` (matches "safe by default"); anyone
  currently depending on XFF being honored was already exposed to the spoofing bug
  described in SEC-2, so this is a deliberate breaking-for-safety change, consistent
  with the master plan's intent.
- **Risk:** No automated test currently covers `client_ip()` behavior or rate-limiter
  pruning. **Mitigation:** Will consider adding a focused unit test for both during
  implementation/review if it fits cleanly without expanding scope.

## Files

- `crates/vexboard-server/src/config.rs` (add `behind_proxy` field)
- `crates/vexboard-server/src/api/auth.rs:24-35` (`client_ip`, `login()` call site)
- `crates/vexboard-server/src/rate_limit.rs` (prune empty entries)
