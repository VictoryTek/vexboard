# Spec: Fix unused `serde::Deserialize` import warning in auth.rs

## Current State Analysis
`crates/vexboard-server/src/api/auth.rs:8` has an unconditional
`use serde::Deserialize;`. The only consumer of `Deserialize` in this file is
the `UpdateMeRequest` struct (line 38-43), which is gated behind
`#[cfg(not(all(unix, feature = "pam-auth")))]`.

The nix package build (`nix/package.nix:48`) builds with
`--features pam-auth` on Linux. With that feature enabled, the cfg condition
is false, `UpdateMeRequest` is not compiled, and the `Deserialize` import
becomes unused — producing the reported warning. This does not reproduce in a
default (`cargo build --release --bin vexboard-server`, no features) build,
which is why it wasn't caught locally.

## Problem Definition
Unused-import warning during `pam-auth`-featured release builds:
```
warning: unused import: `serde::Deserialize`
 --> crates/vexboard-server/src/api/auth.rs:8:5
```

## Proposed Solution
Gate the `use serde::Deserialize;` import with the same `#[cfg(...)]`
attribute as its sole consumer, so it's only compiled in when
`UpdateMeRequest` is compiled in.

## Implementation Steps
1. In `crates/vexboard-server/src/api/auth.rs`, change:
   ```rust
   use serde::Deserialize;
   ```
   to:
   ```rust
   #[cfg(not(all(unix, feature = "pam-auth")))]
   use serde::Deserialize;
   ```

## Dependencies
None — no new crates, no Context7 lookup required (internal-only change, no
external library integration).

## Configuration Changes
None.

## Risks and Mitigations
- Risk: cfg-gating the import incorrectly could cause a "used but not
  imported" error for other cfg combinations. Mitigated by matching the
  cfg attribute exactly to `UpdateMeRequest`'s own gate, and by verifying no
  other item in the file uses `Deserialize` directly (confirmed via read of
  full file).
