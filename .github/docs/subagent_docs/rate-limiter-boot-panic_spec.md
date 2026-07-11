# SEC-4 — Login Rate Limiter Boot-Time Panic — Spec

## Current State Analysis

`LoginRateLimiter::check` (crates/vexboard-server/src/rate_limit.rs:25-41) computes:

```rust
let now = Instant::now();
let cutoff = now - self.window;
```

`Instant` subtraction panics on underflow (`Instant - Duration` uses `Instant::sub`, which
panics rather than saturating, per `std::time::Instant` docs: "This function may panic if the
resulting point in time cannot be represented"). On platforms/environments where the process's
monotonic clock origin is close to zero (e.g. some containerized/boot-time scenarios) and
`window` (from `login_rate_limit_window_secs` config, default window) exceeds elapsed time
since that origin, `now - self.window` underflows and panics inside the `login` handler,
taking down that request (and, depending on panic behavior, potentially the worker/whole
process if not caught by axum's panic-catching middleware — regardless, an avoidable panic on
a security-critical path is unacceptable).

`check()` is called from `crates/vexboard-server/src/api/auth.rs:80` in the `login` handler,
directly in the hot path before any credential check.

## Problem Definition

Any login attempt within `window` duration of process start on an affected platform panics the
handler instead of correctly returning "allowed" (no attempts could have been recorded yet
regardless).

## Proposed Solution

Replace the unchecked subtraction with `Instant::checked_sub`, which returns `None` on
underflow instead of panicking. When `checked_sub` returns `None`, no meaningful cutoff can
exist yet (process hasn't been up long enough for anything to expire), so treat it the same as
the earliest possible cutoff — i.e. keep every existing entry (nothing to prune) by using
`Instant::now()` question doesn't apply; simplest correct behavior: if `now.checked_sub(window)`
is `None`, no attempt could be older than the window yet, so skip pruning entirely for this
call (equivalent semantically to a cutoff of "epoch", i.e. nothing is evicted).

## Implementation Steps

1. In `crates/vexboard-server/src/rate_limit.rs:26-32`, change:
   ```rust
   let now = Instant::now();
   let cutoff = now - self.window;
   let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
   let attempts = state.entry(ip).or_default();
   while attempts.front().is_some_and(|t| *t < cutoff) {
       attempts.pop_front();
   }
   ```
   to:
   ```rust
   let now = Instant::now();
   let cutoff = now.checked_sub(self.window);
   let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
   let attempts = state.entry(ip).or_default();
   if let Some(cutoff) = cutoff {
       while attempts.front().is_some_and(|t| *t < cutoff) {
           attempts.pop_front();
       }
   }
   ```
   When `cutoff` is `None` (process hasn't been alive as long as `window`), no pruning happens
   for that call — correct, since nothing could be older than `window` yet anyway.

## Dependencies

None new — `Instant::checked_sub` is stable std.

## Configuration Changes

None.

## Risks and Mitigations

- **Risk:** Skipping pruning during the boot-time grace period could let `attempts` briefly
  hold more entries than the window would strictly allow.
  **Mitigation:** Not possible — if `now` is younger than `window`, every recorded attempt is
  by definition inside the window already, so there is nothing to prune. The set of "attempts
  that should be pruned" is empty in this state.
- **Risk:** None to existing passing tests — the change only affects behavior in the narrow
  `now < window` case, which none of the existing tests construct (they run well after process
  start in a normal test run... actually re-examine: `Instant::now()` in a test binary is also
  relative to an arbitrary origin, so this **could** in principle affect tests too, but in
  practice test binaries run long enough after process start that `now >= window` (60s) holds).

## Test Plan

Add a unit test that directly exercises `checked_sub` returning `None` is hard to do
deterministically without mocking `Instant` (not natively mockable in std). Given the fix is a
narrow, provably-correct guard against a documented panic condition, and existing tests
(`blocks_after_max_attempts_within_window`, `distinct_ips_have_independent_budgets`,
`rate_limited_call_with_no_prior_attempts_prunes_empty_entry`) continue to cover the normal-path
pruning/eviction logic unchanged, no new test is added — this matches the project's own stated
fix guidance ("Use `now.checked_sub(self.window)`") without introducing disproportionate mock
infrastructure for `Instant`.
