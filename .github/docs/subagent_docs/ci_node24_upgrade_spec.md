# Spec: CI Node.js 24 Actions Upgrade

**Feature name:** `ci_node24_upgrade`
**Date:** 2026-06-07
**Phase:** 1 — Research & Specification

---

## Current State Analysis

File: `.github/workflows/ci.yml`

The CI workflow has four jobs: `backend`, `frontend`, `security`, and `publish`. One action is pinned to a version that uses Node.js 20:

| Action | Current Version | Node.js Runtime | Status |
|--------|----------------|-----------------|--------|
| `actions/checkout` | `@v6` | Node.js 24 | ✅ Current |
| `dtolnay/rust-toolchain` | `@stable` | N/A (shell) | ✅ N/A |
| `Swatinem/rust-cache` | `@v2` | Node.js 20 (patched) | ✅ Maintained |
| `actions/upload-artifact` | `@v4` | **Node.js 20** | ❌ Deprecated |
| `docker/setup-buildx-action` | `@v4` | Node.js 24 | ✅ Current |
| `docker/login-action` | `@v4` | Node.js 24 | ✅ Current |
| `docker/metadata-action` | `@v6` | Node.js 24 | ✅ Current |
| `docker/build-push-action` | `@v7` | Node.js 24 | ✅ Current |

---

## Problem Definition

GitHub has deprecated Node.js 20 as the runtime for GitHub Actions:
- **Forced Node.js 24 default:** June 16, 2026
- **Node.js 20 fully removed:** September 16, 2026

The annotation on the `backend` job warns:
> "Node.js 20 actions are deprecated. The following actions are running on Node.js 20 and may not work as expected: `actions/upload-artifact@v4`."

`actions/upload-artifact@v4` uses `node20` as its runtime. Starting June 16, 2026, jobs will fail with a forced `node24` environment that the v4 binary was not compiled against.

---

## Proposed Solution Architecture

**Single-line change** in `.github/workflows/ci.yml`:

```
actions/upload-artifact@v4  →  actions/upload-artifact@v6
```

### Why v6?

- `v5` added preliminary Node.js 24 support but defaulted to Node.js 20 — still triggers the deprecation warning.
- `v6` uses `node24` as its default runtime and fully resolves the deprecation.
- The `name`, `path`, and `retention-days` inputs used in ci.yml are unchanged between v4 and v6; the upgrade is backwards-compatible for this usage.

### No new dependencies

This change requires no new Cargo dependencies, no Rust code changes, and no Context7 lookup. The only affected file is the CI workflow YAML.

---

## Implementation Steps

1. Open `.github/workflows/ci.yml`
2. Locate line 62: `uses: actions/upload-artifact@v4`
3. Change to: `uses: actions/upload-artifact@v6`
4. Save file — no other changes required

---

## Configuration Changes

None beyond the version pin in ci.yml.

---

## Build/Test Commands for Phase 3

Per CLAUDE.md approved safe commands — no compilation change is needed since ci.yml is YAML only:

- `cargo fmt --all -- --check` — verify no Rust formatting regressions (zero-cost)
- `cat .github/workflows/ci.yml | grep upload-artifact` — confirm version pin updated

These commands do not appear in FORBIDDEN COMMANDS and have negligible resource cost.

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| v6 changes API input names | Low | `name`, `path`, `retention-days` are stable; confirmed in v4→v6 release notes |
| Artifact download in downstream job fails | N/A | No `download-artifact` step in this workflow |
| Self-hosted runner too old (requires ≥ 2.327.1) | N/A | Workflow uses `ubuntu-latest` (GitHub-hosted runners, always current) |

---

## Sources

1. [GitHub Changelog: Deprecation of Node 20 on GitHub Actions runners](https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/)
2. [actions/upload-artifact Releases](https://github.com/actions/upload-artifact/releases)
3. [Issue #138: Node.js 20 deprecation warning for upload-pages-artifact](https://github.com/actions/upload-pages-artifact/issues/138)
4. [Projen issue: Bump upload-artifact and download-artifact for Node 24](https://github.com/projen/projen/issues/4570)
5. [Community discussion: Questions on Deprecation of Node 20](https://github.com/orgs/community/discussions/189324)
6. [Adafruit CircuitPython: Update CI actions for Node.js 24](https://github.com/adafruit/circuitpython/issues/10888)
