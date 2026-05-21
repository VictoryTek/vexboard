# Alpine Dockerfile Migration — Review

**Feature**: Migrate multi-stage Dockerfile from Debian bookworm-slim to Alpine Linux  
**Date**: 2026-05-21  
**Reviewer**: Review Subagent (static analysis)  
**File reviewed**: `Dockerfile`  
**Spec reference**: `.github/docs/subagent_docs/alpine_dockerfile_spec.md`

---

## Reviewed Dockerfile (full text)

```dockerfile
# Stage 1: Build Rust backend
FROM rust:1.85-alpine AS backend-builder
RUN apk add --no-cache build-base cmake perl bash pkgconf openssl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release --bin vexboard-server

# Stage 2: Build frontend (Trunk + WASM)
FROM rust:1.85-alpine AS frontend-builder
RUN apk add --no-cache build-base cmake perl bash pkgconf openssl-dev && \
    rustup target add wasm32-unknown-unknown && \
    cargo install trunk
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
WORKDIR /build/crates/vexboard-frontend
RUN trunk build --release

# Stage 3: Runtime
FROM alpine:3.21
RUN apk add --no-cache openssl ca-certificates
WORKDIR /app
COPY --from=backend-builder /build/target/release/vexboard-server ./vexboard
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
RUN mkdir -p /var/lib/vexboard
EXPOSE 7280
ENTRYPOINT ["./vexboard"]
```

---

## Checklist Validation

| # | Criterion | Result | Notes |
|---|-----------|--------|-------|
| 1 | All three stages use Alpine base images | ✅ PASS | `rust:1.85-alpine` × 2, `alpine:3.21` × 1 |
| 2 | No `apt-get` calls remain | ✅ PASS | Zero occurrences of `apt-get` |
| 3 | Backend builder: `build-base cmake perl bash pkgconf openssl-dev` | ✅ PASS | Exact match, single `apk add --no-cache` call |
| 4 | Frontend builder: same packages + `rustup target add` + `cargo install trunk` | ✅ PASS | All three chained in one `RUN` layer |
| 5 | Runtime: `openssl ca-certificates` | ✅ PASS | `apk add --no-cache openssl ca-certificates` |
| 6 | WORKDIR / COPY / RUN / EXPOSE / ENTRYPOINT correct | ✅ PASS | All instructions logically sound and complete |
| 7 | COPY paths match spec | ✅ PASS | `/build/target/release/vexboard-server → ./vexboard`; `/build/crates/vexboard-frontend/dist → ./assets` |
| 8 | No Debian artifacts remain | ✅ PASS | No `apt-get`, `dpkg`, Debian package names, or `debian:`/`slim` references |
| 9 | `--no-cache` on all `apk add` calls | ✅ PASS | Present on all three `apk add` invocations |
| 10 | No hardcoded secrets, no overly broad permissions | ✅ PASS | No secrets; no `chmod 777` or similar |

**All 10 checklist items PASS.**

---

## Deviations from Spec

### Minor: RUN layer consolidation in frontend-builder (NOT a defect)

The spec shows `apk add` and `rustup + trunk` as two separate `RUN` instructions:

```dockerfile
# Spec
RUN apk add --no-cache ...
RUN rustup target add wasm32-unknown-unknown && cargo install trunk
```

The implementation chains all three into one `RUN`:

```dockerfile
# Implementation
RUN apk add --no-cache ... && \
    rustup target add wasm32-unknown-unknown && \
    cargo install trunk
```

**Assessment**: This is an improvement over the spec. Fewer Docker layers reduces final image size and is idiomatic practice. Functionally equivalent. Not a defect.

---

## CRITICAL Issues

**None.**

---

## RECOMMENDED Improvements

These are not blocking but should be addressed in a follow-up:

### REC-1: Add a non-root user for the runtime stage (Security)

The runtime container runs as `root`. For a dashboard server exposed on a network port this is unnecessary privilege.

```dockerfile
# Stage 3: Runtime
FROM alpine:3.21
RUN apk add --no-cache openssl ca-certificates && \
    addgroup -S vexboard && adduser -S -G vexboard vexboard
WORKDIR /app
COPY --from=backend-builder /build/target/release/vexboard-server ./vexboard
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
RUN mkdir -p /var/lib/vexboard && chown -R vexboard:vexboard /app /var/lib/vexboard
USER vexboard
EXPOSE 7280
ENTRYPOINT ["./vexboard"]
```

Note: D-Bus socket access (`/run/dbus/system_bus_socket`) may require the user to belong to an appropriate group if running in Docker with the socket mounted.

### REC-2: Pin `trunk` version for reproducible builds (Performance / Consistency)

`cargo install trunk` installs the latest published version of Trunk at build time. If trunk releases a breaking change, CI builds may silently diverge.

```dockerfile
RUN cargo install trunk --version 0.21.14 --locked
```

### REC-3: Consider Cargo dependency layer caching (Performance)

Currently any source file change invalidates the entire `cargo build` cache layer. Using `cargo-chef` (or a dummy-build pattern) separates dependency compilation from application source compilation, dramatically reducing incremental build times in CI:

```dockerfile
FROM rust:1.85-alpine AS chef
RUN apk add --no-cache build-base cmake perl bash pkgconf openssl-dev
RUN cargo install cargo-chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS backend-builder
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin vexboard-server
```

This is particularly valuable in GitHub Actions where build minutes are billable.

---

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 98% | A+ |
| Best Practices | 85% | B+ |
| Functionality | 98% | A+ |
| Code Quality | 95% | A |
| Security | 82% | B |
| Performance | 72% | C+ |
| Consistency | 100% | A+ |
| Build Success | 95% | A |

**Overall Grade: A- (91%)**

> Build Success is scored at 95% (not 100%) because actual Docker image build execution was not performed — static analysis only, per reviewer instructions. All instructions are syntactically correct and logically consistent with the spec.

---

## Summary

The Dockerfile migration from Debian `bookworm-slim` / `rust:1.85-slim` to Alpine (`rust:1.85-alpine` / `alpine:3.21`) is **complete and correct**. All 10 checklist criteria pass. The implementation matches the spec exactly on every functional point: base images, package sets, WASM toolchain installation, COPY paths, WORKDIR structure, EXPOSE, and ENTRYPOINT are all correct.

The one structural deviation — chaining `apk add`, `rustup target add`, and `cargo install trunk` into a single `RUN` layer in the frontend builder — is a net improvement over the spec (fewer layers, smaller intermediate image).

No Debian artifacts remain. No hardcoded secrets are present.

**Result: PASS**

Three recommended (non-blocking) improvements were identified: non-root runtime user (security hardening), pinned trunk version (reproducibility), and Cargo dependency layer caching via `cargo-chef` (CI build time). None of these constitute a defect in the current implementation.
