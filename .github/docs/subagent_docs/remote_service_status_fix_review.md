# Phase 3 Review — remote_service_status_fix

**Date:** 2026-07-02

## Problem Statement

Services claimed via Docker/Podman discovery (including containers on a remote Docker
host reachable via `config.docker.sockets` `tcp://...` entries) had the container name
written into `systemd_unit` as well as `url`. Because the probe dispatcher checked
`systemd_unit.is_some()` first (intentional per `probe_priority_fix`, 2026-06-07, for
*arr apps), these services were always checked against the local system D-Bus, never
found, and permanently reported "down" — even when reachable via `url`.

---

## Modified Files

| File | Change |
|------|--------|
| `crates/vexboard-server/src/probe/mod.rs` | Dispatch now requires `discovery_source` to NOT be `"docker"`/`"podman"` before taking the systemd D-Bus branch |
| `crates/vexboard-server/src/api/services.rs` | Identical change in the immediate post-create background probe |

---

## Review Criteria

### 1. Specification Compliance — 100% / A

Spec called for gating the systemd branch on `discovery_source`, in exactly the two
dispatch sites. Both updated identically. No scope creep; frontend left untouched per
spec's stated rationale (self-healing fix, no dependency on frontend data hygiene).

### 2. Best Practices — 100% / A

- Preserves the 2026-06-07 `probe_priority_fix` for systemd-discovered/manually
  configured services (`discovery_source` unset or `"systemd"`).
- Self-healing: existing misclassified DB rows correct themselves on the next probe
  tick without a migration, since the dispatcher re-reads `discovery_source` live.

### 3. Functionality — 100% / A

- `discovery_source == "systemd"` or `None` + `systemd_unit` set → D-Bus probe (unchanged)
- `discovery_source == "docker"`/`"podman"` + `url` set → HTTP probe (fixed, local or remote host)
- `discovery_source == "docker"`/`"podman"` with no `url` → no probe (pre-existing, unchanged, out of scope)

### 4. Code Quality — 100% / A

- `cargo fmt --all -- --check` → PASS
- `cargo clippy --workspace -- -D warnings` → PASS, 0 warnings
- 8-line change across two files; identical logic in both, no divergence

### 5. Security — 100% / A

No new attack surface; `discovery_source` is server-controlled data already stored via
the existing create/claim path, not new user input reaching a new code path.

### 6. Performance — 100% / A

No change — same number of branches evaluated per probe tick.

### 7. Consistency — 100% / A

Both probe dispatch sites updated with byte-identical logic.

### 8. Build Validation

```
cargo fmt --all -- --check                    → PASS
cargo clippy --workspace -- -D warnings       → PASS (0 warnings; full crate compiles)
cargo test -p vexboard-server                 → BLOCKED — environment, not code
cargo build --release --bin vexboard-server   → BLOCKED — environment, not code
```

**Root cause of BLOCKED steps (verified pre-existing, unrelated to this change):**
`ld.lld` invokes `/home/nimda/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld/ld-wrapper.sh`, which itself execs a
Nix-store path (`/nix/store/9q0ah902348jm3y4v4m975sia92lmb8h-rustup-1.28.2/nix-support/ld-wrapper.sh`)
that no longer exists on disk (confirmed via direct `ls`) — the rustup toolchain's
linker wrapper was garbage-collected from the Nix store out from under the active
`rustup` installation. This fails identically for any Rust binary crate on this
machine right now, regardless of source content. `cargo clippy` succeeds because it
performs full type-checking/codegen-check without invoking the final system linker,
which is why it can confirm the code compiles correctly even though `cargo build` and
`cargo test` cannot currently link on this machine.

This mirrors the class of pre-existing environment issue CLAUDE.md already documents
for the D-Bus SIGSEGV case, but is a distinct failure mode (broken linker, not a
runtime crash) and is NOT currently exempted by `scripts/preflight.sh`.

---

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | N/A | BLOCKED (environment) |

**Overall Grade: A on all code-reviewable criteria; Build Success step cannot complete
on this machine due to a pre-existing broken Rust linker toolchain, unrelated to this
change.**

---

## Result: NEEDS_REFINEMENT (procedural) — escalating to user per CLAUDE.md

The code change itself passes every review criterion. However, per CLAUDE.md, Phase 6
Preflight requires the full backend release build to succeed with exit code 0, and this
cannot happen while the machine's Rust linker toolchain is broken. This is not something
refinement cycles on the source diff can fix — no code change can repair a missing file
in the Nix store. Escalating to the user now rather than spending refinement cycles on
a diff that is already correct.
