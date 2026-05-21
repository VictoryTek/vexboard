# GHCR Publish Workflow — Specification

**Feature:** Add Docker build-and-push CI job publishing to GitHub Container Registry (GHCR)  
**Date:** 2026-05-21  
**Author:** Research Subagent  
**Status:** READY FOR IMPLEMENTATION

---

## 1. Sources Consulted

| # | Source | URL | Relevance |
|---|--------|-----|-----------|
| 1 | GitHub Docs — Publishing Docker images to GitHub Packages | https://docs.github.com/en/actions/use-cases-and-examples/publishing-packages/publishing-docker-images | Official workflow template for GHCR push using `GITHUB_TOKEN` |
| 2 | GitHub Docs — Working with the Container registry | https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry | Authentication requirements, `packages: write` permission, `org.opencontainers.image.source` label |
| 3 | `docker/metadata-action` README (v6) | https://github.com/docker/metadata-action | Tag type reference (`type=raw`, `type=sha`, `{{is_default_branch}}`), `images` input, `labels` output |
| 4 | `docker/build-push-action` README (v7) | https://github.com/docker/build-push-action | `cache-from`/`cache-to`, `context`, `push`, `tags`, `labels` inputs; Buildx setup requirement |
| 5 | Docker Docs — Cache management with GitHub Actions | https://docs.docker.com/build/ci/github-actions/cache/ | `type=gha` cache backend (requires Buildx ≥ 0.21.0, GHA Cache API v2; GitHub-hosted runners satisfy this); `docker/setup-buildx-action@v4` prerequisite |
| 6 | Docker Docs — Multi-stage builds | https://docs.docker.com/build/building/multi-stage/ | Workspace-context COPY patterns; why missing workspace root in a stage causes unreproducible builds |

---

## 2. Current State Analysis

### 2.1 Existing CI Workflow (`ci.yml`)

Three jobs are defined:

| Job | Purpose | Runs on |
|-----|---------|---------|
| `backend` | fmt, clippy, test, `cargo build --release --bin vexboard-server` | push + PR to main/master |
| `frontend` | `trunk build --release` (WASM) | push + PR to main/master |
| `security` | `cargo audit` via `rustsec/audit-check@v2` | push + PR to main/master |

**Missing:** No job builds or pushes a Docker image anywhere.

### 2.2 `docker-compose.yml` Breakage

```yaml
image: ghcr.io/victorytek/vexboard:latest
```

`docker compose up -d` fails with a **"denied"** error because:
1. No image has ever been pushed to `ghcr.io/victorytek/vexboard`.
2. GHCR returns HTTP 401/403 ("denied") when pulling a non-existent or private package.
3. The `build:` key in the Compose file only rebuilds locally if `docker compose up --build` is used explicitly; the default `up -d` tries to pull first and fails.

### 2.3 Dockerfile Stage 2 — Missing Workspace Context

**Current Stage 2:**

```dockerfile
FROM rust:1.85-slim AS frontend-builder
RUN rustup target add wasm32-unknown-unknown && \
    cargo install trunk
WORKDIR /frontend
COPY crates/vexboard-frontend/ ./
RUN trunk build --release
```

**Root cause of the bug:**

- The workspace `Cargo.toml` (at repo root) defines `resolver = "2"` and the `[workspace]` members block.
- The `Cargo.lock` (at repo root) pins all transitive dependency versions.
- Stage 2 copies **only** `crates/vexboard-frontend/` to `/frontend/`. Neither `Cargo.toml` nor `Cargo.lock` from the workspace root is present.
- When `trunk build --release` invokes `cargo build --target wasm32-unknown-unknown`, Cargo walks up from `/frontend` looking for a workspace root. It finds nothing above `/frontend`, so it treats `/frontend/Cargo.toml` as a standalone (non-workspace) crate.
- Because there is no `Cargo.lock` in `/frontend`, Cargo generates a **fresh lock file** during the build. This makes the Docker image build **non-deterministic**: dependency versions may differ from what the `cargo test` job in CI verified.
- If any `vexboard-frontend` dependency adds a breaking patch between CI runs, Stage 2 silently picks it up while the test jobs saw the old version.

**Secondary impact:** `COPY --from=frontend-builder /frontend/dist ./assets` in Stage 3 will correctly refer to the compiled output, but once the WORKDIR changes in the fix below the path changes.

---

## 3. Problem Definition

| # | Problem | Severity |
|---|---------|----------|
| P1 | No GitHub Actions job exists to build or push the Docker image to GHCR | **Critical** — breaks `docker compose up -d` for all users |
| P2 | Dockerfile Stage 2 does not include workspace `Cargo.toml` / `Cargo.lock`, making builds non-deterministic | **High** — silent divergence between tested and shipped dependency versions |

---

## 4. Proposed Solution Architecture

### 4.1 New `publish` Job in `ci.yml`

**Trigger logic:**

The current workflow triggers on:
```yaml
on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]
```

Because `pull_request` is also a trigger, the `publish` job **must** carry `if: github.event_name == 'push'` to prevent it from running on PRs (which would expose `GITHUB_TOKEN` write access to PR-triggered builds from forks, a well-known supply-chain risk).

The `needs: [backend, frontend]` gate ensures Docker is only built and pushed when both the backend and frontend CI jobs pass.

**Actions used (all latest stable as of 2026-05-21):**

| Action | Version | Purpose |
|--------|---------|---------|
| `actions/checkout` | `v4` | Checks out source with full history for metadata |
| `docker/setup-buildx-action` | `v4` | Creates a BuildKit builder; required for `type=gha` cache backend |
| `docker/login-action` | `v4` | Authenticates against `ghcr.io` using `GITHUB_TOKEN` |
| `docker/metadata-action` | `v6` | Produces `tags` and `labels` from Git context |
| `docker/build-push-action` | `v7` | Builds using BuildKit, pushes to GHCR, uses GHA cache |

**Tagging strategy:**

| Tag type | Produced tag | When |
|----------|-------------|------|
| `type=raw,value=latest,enable={{is_default_branch}}` | `ghcr.io/victorytek/vexboard:latest` | Any push to `main` or `master` |
| `type=sha,prefix=sha-` | `ghcr.io/victorytek/vexboard:sha-<7-char-sha>` | Every push |

The `{{is_default_branch}}` expression from `metadata-action` evaluates to `true` when the current branch is the repository's default branch, which is cleaner than hardcoding `main` or `master`.

**Caching strategy:**

`type=gha` (GitHub Actions cache backend, BuildKit native API) is used. This:
- Requires Buildx ≥ 0.21.0 (GitHub-hosted Ubuntu runners satisfy this as of April 2025).
- Uses GHA Cache API v2 (the legacy v1 was shut down April 15, 2025).
- `mode=max` exports all intermediate stage caches, significantly speeding up Rust multi-stage builds where the backend and frontend compile independently.

### 4.2 Dockerfile Stage 2 Fix

Restructure Stage 2 to mirror Stage 1: copy the workspace root files first, then all crates, then run Trunk from the frontend crate subdirectory.

**Before:**
```dockerfile
WORKDIR /frontend
COPY crates/vexboard-frontend/ ./
RUN trunk build --release
```

**After:**
```dockerfile
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
WORKDIR /build/crates/vexboard-frontend
RUN trunk build --release
```

The `dist/` output from Trunk lands at `/build/crates/vexboard-frontend/dist/`.

**Stage 3 COPY must also be updated:**

**Before:**
```dockerfile
COPY --from=frontend-builder /frontend/dist ./assets
```

**After:**
```dockerfile
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
```

---

## 5. Exact YAML Changes for `ci.yml`

### 5.1 Optional — Top-Level `permissions` Block (Recommended Hardening)

Add a restrictive default at the top of `ci.yml` (after `env:`), then grant `packages: write` only in the `publish` job. This follows the GitHub principle of least privilege:

```yaml
# Restrict default GITHUB_TOKEN permissions for all jobs
permissions:
  contents: read
```

This is **optional but recommended**. Without it the default token still works, but defense-in-depth is preferred.

### 5.2 The Complete New `publish` Job

Insert after the `security` job:

```yaml
  publish:
    name: Publish (GHCR)
    runs-on: ubuntu-latest
    needs: [backend, frontend]
    if: github.event_name == 'push'
    permissions:
      contents: read
      packages: write

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v4

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@v4
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract Docker metadata (tags · labels)
        id: meta
        uses: docker/metadata-action@v6
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=raw,value=latest,enable={{is_default_branch}}
            type=sha,prefix=sha-

      - name: Build and push Docker image
        uses: docker/build-push-action@v7
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

**Note on `if:` vs workflow trigger:**
The workflow `on:` block includes both `push` and `pull_request`. Adding `if: github.event_name == 'push'` at the job level is the correct place to filter — it does not need to be on individual steps. The workflow-level trigger alone is insufficient because it allows both event types through to all jobs.

### 5.3 Complete Revised `ci.yml`

For reference, the implementation subagent should produce this complete file:

```yaml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

# Restrict default token permissions; publish job overrides as needed
permissions:
  contents: read

jobs:
  backend:
    name: Backend (build · lint · test)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo registry & build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Clippy (warnings as errors)
        run: cargo clippy --workspace -- -D warnings

      - name: Run tests
        run: cargo test --workspace

      - name: Build backend (release)
        run: cargo build --release --bin vexboard-server

  frontend:
    name: Frontend (WASM build)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable + wasm32 target
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Install Trunk
        uses: jetli/trunk-action@v0.5.0
        with:
          version: latest

      - name: Cache cargo registry & build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-wasm-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-wasm-

      - name: Build frontend (release)
        working-directory: crates/vexboard-frontend
        run: trunk build --release

  security:
    name: Security audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

  publish:
    name: Publish (GHCR)
    runs-on: ubuntu-latest
    needs: [backend, frontend]
    if: github.event_name == 'push'
    permissions:
      contents: read
      packages: write

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v4

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@v4
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract Docker metadata (tags · labels)
        id: meta
        uses: docker/metadata-action@v6
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=raw,value=latest,enable={{is_default_branch}}
            type=sha,prefix=sha-

      - name: Build and push Docker image
        uses: docker/build-push-action@v7
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

---

## 6. Dockerfile Fix

### 6.1 Complete Revised Stage 2

Replace the existing Stage 2 block:

**Before:**
```dockerfile
# Stage 2: Build frontend (Trunk + WASM)
FROM rust:1.85-slim AS frontend-builder
RUN rustup target add wasm32-unknown-unknown && \
    cargo install trunk
WORKDIR /frontend
COPY crates/vexboard-frontend/ ./
RUN trunk build --release
```

**After:**
```dockerfile
# Stage 2: Build frontend (Trunk + WASM)
FROM rust:1.85-slim AS frontend-builder
RUN rustup target add wasm32-unknown-unknown && \
    cargo install trunk
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
WORKDIR /build/crates/vexboard-frontend
RUN trunk build --release
```

### 6.2 Stage 3 `COPY` Update

**Before:**
```dockerfile
COPY --from=frontend-builder /frontend/dist ./assets
```

**After:**
```dockerfile
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
```

### 6.3 Complete Revised Dockerfile (for reference)

```dockerfile
# Stage 1: Build Rust backend
FROM rust:1.85-slim AS backend-builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release --bin vexboard-server

# Stage 2: Build frontend (Trunk + WASM)
FROM rust:1.85-slim AS frontend-builder
RUN rustup target add wasm32-unknown-unknown && \
    cargo install trunk
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
WORKDIR /build/crates/vexboard-frontend
RUN trunk build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /build/target/release/vexboard-server ./vexboard
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
RUN mkdir -p /var/lib/vexboard
EXPOSE 7280
ENTRYPOINT ["./vexboard"]
```

---

## 7. Implementation Steps

The implementation subagent must perform the following changes in order:

### Step 1 — Fix `Dockerfile` (Stage 2 + Stage 3)

Edit `Dockerfile`:

1. In Stage 2 (`frontend-builder`), replace:
   ```
   WORKDIR /frontend
   COPY crates/vexboard-frontend/ ./
   ```
   with:
   ```
   WORKDIR /build
   COPY Cargo.toml Cargo.lock ./
   COPY crates/ ./crates/
   WORKDIR /build/crates/vexboard-frontend
   ```

2. In Stage 3 (runtime), replace:
   ```
   COPY --from=frontend-builder /frontend/dist ./assets
   ```
   with:
   ```
   COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
   ```

### Step 2 — Update `ci.yml`

Edit `.github/workflows/ci.yml`:

1. Add a top-level `permissions:` block with `contents: read` (after the `env:` block).
2. Append the `publish` job (as specified in §5.2) after the `security` job.

### Step 3 — Verify `Trunk.toml` Output Directory

Open `crates/vexboard-frontend/Trunk.toml`. Confirm the `dist` directory is not customized to a non-default path. The default Trunk output is `dist/` relative to the workspace root of the Trunk project. With `WORKDIR /build/crates/vexboard-frontend`, this means `/build/crates/vexboard-frontend/dist`. If `Trunk.toml` overrides `dist`, adjust the Stage 3 COPY path accordingly.

### Step 4 — No `docker-compose.yml` Changes Required

`docker-compose.yml` already references `ghcr.io/victorytek/vexboard:latest`. Once the `publish` job runs and pushes that tag, `docker compose up -d` will succeed without any Compose file changes.

---

## 8. Files Modified

| File | Change |
|------|--------|
| `.github/workflows/ci.yml` | Add `permissions: contents: read` at workflow level; add `publish` job |
| `Dockerfile` | Fix Stage 2 WORKDIR/COPY; update Stage 3 COPY path |

---

## 9. Risks and Mitigations

| Risk | Likelihood | Severity | Mitigation |
|------|-----------|----------|------------|
| First push after merge: GHCR package is private by default | High | Medium | After first push, navigate to `github.com/{owner}/vexboard/packages`, find `vexboard`, change visibility to **Public** (or keep private and ensure Compose `image:` pull is authenticated via `docker login ghcr.io`). |
| `github.repository` returns mixed-case owner name | Low | Low | `metadata-action` automatically lowercases image names per Docker tag specification. No manual intervention needed. |
| `type=gha` cache unavailable on self-hosted runners | Medium | Low | If a self-hosted runner is added later, the `cache-from/cache-to` lines will silently no-op (no build failure); add `docker/setup-buildx-action@v4 with: version: latest` to force latest Buildx. |
| Trunk output directory differs from `dist/` | Low | High | Verify `Trunk.toml` before finalizing Stage 3 COPY path (see Step 3 above). |
| `cargo install trunk` in Stage 2 is slow and not cached | Medium | Medium | For future optimization, pin a specific Trunk version and use a pre-built binary. This is a performance concern, not a correctness one; out of scope for this change. |
| PR from fork triggers `publish` job | Low (mitigated by `if:`) | High | The `if: github.event_name == 'push'` condition prevents the job from running on `pull_request` events, including those from forks. `GITHUB_TOKEN` write access is never exposed to fork PRs. |
| Docker image build fails inside GitHub Actions due to missing `wasm32` target | Very Low | High | The Dockerfile installs `wasm32-unknown-unknown` via `rustup target add` inside the `frontend-builder` stage itself, independent of the host runner. No runner-level toolchain is required. |

---

## 10. Dependency Verification (Context7)

No new Rust crates or Cargo dependencies are introduced. The changes are limited to:
- GitHub Actions workflow YAML (Docker-provided actions, all versioned tags)
- Dockerfile COPY path corrections

The Docker actions used are the official Docker Inc. actions from the GitHub Marketplace:
- `docker/login-action@v4` — Apache-2.0
- `docker/setup-buildx-action@v4` — Apache-2.0  
- `docker/metadata-action@v6` — Apache-2.0
- `docker/build-push-action@v7` — Apache-2.0

All are the current stable major releases as confirmed via source inspection during research.
