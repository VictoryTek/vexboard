# GHCR Publish — Review Document

**Feature:** Add Docker build-and-push CI job publishing to GitHub Container Registry (GHCR)
**Date:** 2026-05-21
**Reviewer:** Review Subagent
**Status:** PASS

---

## 1. Files Reviewed

| File | Verdict |
|------|---------|
| `.github/workflows/ci.yml` | PASS (minor spec deviations — RECOMMENDED) |
| `Dockerfile` | PASS — matches spec exactly |

Spec validated against: `.github/docs/subagent_docs/ghcr_publish_spec.md`

---

## 2. ci.yml — Detailed Findings

### 2.1 Checklist

| Requirement | Status | Notes |
|-------------|--------|-------|
| `publish` job exists with `needs: [backend, frontend]` | ✅ PASS | Present at correct position |
| `if: github.event_name == 'push'` on `publish` job | ✅ PASS | Prevents PR runs |
| Job-level `permissions: packages: write` | ✅ PASS | Present under `publish` |
| Job-level `permissions: contents: read` | ✅ PASS | Present under `publish` |
| Login step targets `ghcr.io` registry | ✅ PASS | `registry: ghcr.io` |
| Login uses `${{ github.actor }}` | ✅ PASS | |
| Login uses `${{ secrets.GITHUB_TOKEN }}` | ✅ PASS | No PAT used |
| `meta` step has `id: meta` | ✅ PASS | |
| `latest` tag via `type=raw,value=latest,enable={{is_default_branch}}` | ✅ PASS | |
| `sha-<short>` tag via `type=sha,prefix=sha-` | ✅ PASS | `format=short` added — harmless (this is the default) |
| Build-push uses `${{ steps.meta.outputs.tags }}` | ✅ PASS | |
| Build-push uses `${{ steps.meta.outputs.labels }}` | ✅ PASS | |
| `cache-from: type=gha` | ✅ PASS | |
| `cache-to: type=gha,mode=max` | ✅ PASS | |
| `backend` job unchanged | ✅ PASS | All steps identical to pre-change baseline |
| `frontend` job unchanged | ✅ PASS | All steps identical to pre-change baseline |
| `security` job unchanged | ✅ PASS | All steps identical to pre-change baseline |

### 2.2 Deviations from Spec (RECOMMENDED — not CRITICAL)

**1. Action versions are one major version behind the spec:**

| Action | Spec Version | Implemented Version |
|--------|-------------|---------------------|
| `docker/setup-buildx-action` | v4 | v3 |
| `docker/login-action` | v4 | v3 |
| `docker/metadata-action` | v6 | v5 |
| `docker/build-push-action` | v7 | v5 |

Spec §4.1 explicitly states these are "latest stable as of 2026-05-21" and the reference YAML in §5.3 uses v4/v4/v6/v7. The older versions are still functional — `docker/build-push-action@v5` and `docker/metadata-action@v5` are widely deployed. The GHA cache backend (`type=gha`) is driven by Buildx itself, not the action version, so cache API v2 compatibility is unaffected. However, future minor features and bug fixes from v6/v7 will be missed. **Upgrade recommended.**

**2. Hardcoded image path instead of dynamic `github.repository`:**

```yaml
# Implemented
images: ghcr.io/victorytek/vexboard

# Spec §5.2 recommends
images: ghcr.io/${{ github.repository }}
```

The hardcoded path is functionally correct for the current repo but will silently misdirect pushes if the repository is ever renamed or transferred. Using `${{ github.repository }}` is the idiomatic, portable pattern per the spec and GitHub Container Registry docs. **Change recommended for maintainability.**

**3. Missing top-level `permissions: contents: read` block:**

Spec §5.1 recommends adding a workflow-level default:
```yaml
permissions:
  contents: read
```
This restricts all jobs' default token scope under least-privilege. It was marked "optional but recommended" in the spec. The absence is not a functional or security failure since the `publish` job defines its own permissions block, but adding it would prevent future jobs from accidentally inheriting broad default permissions. **Addition recommended.**

---

## 3. Dockerfile — Detailed Findings

### 3.1 Checklist

| Requirement | Status | Notes |
|-------------|--------|-------|
| Stage 1 (`backend-builder`) unchanged | ✅ PASS | Identical to pre-change baseline |
| Stage 2 copies workspace `Cargo.toml` and `Cargo.lock` before build | ✅ PASS | `COPY Cargo.toml Cargo.lock ./` present |
| Stage 2 WORKDIR set to `/build/crates/vexboard-frontend` before `trunk build` | ✅ PASS | |
| Stage 3 copies from `/build/crates/vexboard-frontend/dist` | ✅ PASS | Matches updated Stage 2 output path |
| Stage 3 runtime setup intact | ✅ PASS | `debian:bookworm-slim`, `libssl3`, `ca-certificates`, `mkdir -p /var/lib/vexboard`, `EXPOSE 7280`, `ENTRYPOINT` |
| `Trunk.toml` `dist` path matches Stage 3 COPY source | ✅ PASS | `Trunk.toml` confirms `dist = "dist"` — `/build/crates/vexboard-frontend/dist` is correct |

### 3.2 No Issues Found

The Dockerfile matches the spec reference implementation in §6.3 exactly. The fix is complete and correct: Stage 2 now provides a reproducible, locked build by copying the workspace root `Cargo.toml` and `Cargo.lock` before invoking Trunk, eliminating the non-deterministic fresh lockfile generation described in spec §2.3.

---

## 4. Security Analysis

| Check | Status | Notes |
|-------|--------|-------|
| No hardcoded secrets or tokens | ✅ PASS | |
| `GITHUB_TOKEN` used (not a PAT) | ✅ PASS | |
| `if: github.event_name == 'push'` guards write-scope token from fork PRs | ✅ PASS | Critical security gate present |
| No `pull_request_target` trigger | ✅ PASS | Only `push` and `pull_request` triggers in `on:` block |
| `packages: write` scoped to `publish` job only | ✅ PASS | Other jobs inherit no elevated token scope |

---

## 5. YAML Validity

| Check | Status |
|-------|--------|
| Indentation consistent (2-space throughout) | ✅ PASS |
| No duplicate job names | ✅ PASS (`backend`, `frontend`, `security`, `publish` — all unique) |
| All multi-line strings use correct block scalar (`|`) | ✅ PASS |
| `needs:` references valid job names | ✅ PASS (`backend` and `frontend` both defined) |
| `steps.meta.outputs.tags` expression syntax valid | ✅ PASS |

---

## 6. Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 80% | B |
| Best Practices | 82% | B |
| Functionality | 95% | A |
| Code Quality | 90% | A |
| Security | 92% | A |
| Performance | 95% | A |
| Consistency | 82% | B |
| Build Success | 90% | A |

**Overall Grade: A- (88%)**

---

## 7. Critical Issues

**None.** No issues of critical severity were found.

---

## 8. Recommended Improvements

| Priority | File | Issue |
|----------|------|-------|
| RECOMMENDED | `ci.yml` | Upgrade `docker/setup-buildx-action` v3 → v4 |
| RECOMMENDED | `ci.yml` | Upgrade `docker/login-action` v3 → v4 |
| RECOMMENDED | `ci.yml` | Upgrade `docker/metadata-action` v5 → v6 |
| RECOMMENDED | `ci.yml` | Upgrade `docker/build-push-action` v5 → v7 |
| RECOMMENDED | `ci.yml` | Replace hardcoded `ghcr.io/victorytek/vexboard` with `ghcr.io/${{ github.repository }}` |
| RECOMMENDED | `ci.yml` | Add top-level `permissions: contents: read` for least-privilege hardening |

---

## 9. Summary

Both files implement the core requirements of the spec correctly:

- The `publish` job is properly gated on `needs: [backend, frontend]`, constrained to push events only, uses `GITHUB_TOKEN` correctly, and configures GHA layer caching with `mode=max`.
- The Dockerfile Stage 2 fix resolves the non-deterministic build issue by copying the workspace `Cargo.toml` and `Cargo.lock` before invoking Trunk, and Stage 3 correctly references the updated output path. This is confirmed by `Trunk.toml` which uses the default `dist` directory.
- All three pre-existing CI jobs (`backend`, `frontend`, `security`) are untouched.
- No security vulnerabilities were introduced.

The only deviations from the spec are three RECOMMENDED improvements (action version upgrades, portable image name, global permissions hardening) — none of which prevent the workflow from functioning as intended.

---

## 10. Verdict

**PASS**

The implementation is functionally correct and secure. All critical spec requirements are satisfied. The recommended improvements should be addressed in a follow-up to align fully with the spec's version guidance and best practices, but they do not block delivery.
