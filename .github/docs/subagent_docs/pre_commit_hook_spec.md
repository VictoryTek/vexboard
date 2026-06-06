# Phase 1 Spec: Git Pre-Commit Hook

**Feature:** pre_commit_hook
**Audit Entry:** 2.4.1
**Date:** 2026-06-06

---

## Current State Analysis

- `scripts/preflight.sh` — full CI-equivalent validation (fmt, clippy, tests, release build, audit)
- `scripts/preflight.ps1` — Windows equivalent
- `.git/hooks/` — exists, contains only Git-generated `.sample` files; no active hooks

`cargo fmt --check` and `cargo clippy` are only enforced at CI time (via `scripts/preflight.sh`)
and manually. There is no fast feedback loop at commit time.

---

## Problem Definition

Without a pre-commit hook, formatting and lint violations reach the remote before anyone notices.
The developer then faces a CI failure, context-switches back, and re-commits a fix. A lightweight
pre-commit gate eliminates this cycle.

---

## Design Decisions

### What to run

| Check | Time | Decision |
|-------|------|----------|
| `cargo fmt --all -- --check` | ~1 s (no compile) | ✅ Always run |
| `cargo clippy --workspace -- -D warnings` | ~5–15 s (incremental compile) | ✅ Run by default; skippable via `SKIP_CLIPPY=1` |
| Tests / release build / audit | 30–120 s | ❌ Too slow for a commit hook; left to preflight/CI |

### When to run

Only when at least one `.rs` file is staged. Commits that touch only Markdown, TOML, or frontend
files skip the hook entirely.

### Escape hatch

`SKIP_CLIPPY=1 git commit ...` skips the clippy step for cases where the developer needs to commit
a WIP stub that has known warnings.

### Portability

- Primary: Bash hook for Linux/macOS (`scripts/hooks/pre-commit`)
- Secondary: PowerShell installer for Windows (`scripts/install-hooks.ps1`) that copies the hook
  file rather than symlinking (Git for Windows supports bash scripts in `.git/hooks/` via Git Bash)

### Installation model

Git hooks in `.git/` are not tracked by git. The hook is stored in `scripts/hooks/pre-commit`
(committed to the repo) and activated by running an installer once per developer checkout:
- `scripts/install-hooks.sh` — creates the symlink on Linux/macOS
- `scripts/install-hooks.ps1` — copies the file on Windows

The installer is **not** run automatically (no Cargo build script, no `cargo-husky` dependency).
Developers opt in by running it.

---

## Proposed Solution

### Files to create

1. `scripts/hooks/pre-commit` — the hook script (committed, executable)
2. `scripts/install-hooks.sh` — Linux/macOS installer
3. `scripts/install-hooks.ps1` — Windows installer

### Hook script logic

```
1. Collect staged .rs files
2. If none → exit 0 (nothing to check)
3. Run: cargo fmt --all -- --check
   On failure → print fix hint and exit 1
4. Unless SKIP_CLIPPY=1:
   Run: cargo clippy --workspace -- -D warnings
   On failure → print hint and exit 1
5. exit 0
```

---

## Implementation Steps

1. Create `scripts/hooks/` directory
2. Write `scripts/hooks/pre-commit` hook script
3. Write `scripts/install-hooks.sh`
4. Write `scripts/install-hooks.ps1`
5. Set executable permission on hook and installer scripts

---

## Dependencies

No new Cargo dependencies. Pure shell/PowerShell scripts.

Context7 not required.

---

## Build/Test Commands (Phase 3)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `bash scripts/preflight.sh`
- Manual: `bash scripts/install-hooks.sh` to verify installer runs cleanly
- Manual: verify `.git/hooks/pre-commit` is a symlink pointing to the correct target

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Hook runs expensive commands | Low | Only fmt (1 s) + clippy (incremental); tests/build excluded |
| Staged-only check defeats purpose | None | `cargo fmt --check` checks workspace; a format violation in any staged `.rs` file blocks the commit |
| Windows developers can't use symlink | Low | `install-hooks.ps1` copies instead of symlinks |
| Developers bypass hook | Accepted | `git commit --no-verify` always available; hook is advisory not mandatory |
