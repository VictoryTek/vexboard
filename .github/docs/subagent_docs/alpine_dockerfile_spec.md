# Alpine Dockerfile Migration Specification

**Feature**: Migrate multi-stage Dockerfile from Debian `bookworm-slim` to Alpine Linux  
**Date**: 2026-05-21  
**Status**: Draft

---

## 1. Current State Analysis

### Existing Dockerfile (three stages)

| Stage | Base Image | Purpose |
|-------|-----------|---------|
| `backend-builder` | `rust:1.85-slim` (Debian, glibc) | Compile Axum/Tokio server binary |
| `frontend-builder` | `rust:1.85-slim` (Debian, glibc) | Install Trunk, build Leptos WASM SPA |
| runtime | `debian:bookworm-slim` (glibc) | Run the server binary + serve static assets |

### Current Debian package usage

| Stage | Packages | Purpose |
|-------|---------|---------|
| backend-builder | `pkg-config libssl-dev` | pkg-config tooling; OpenSSL headers for openssl-sys / C builds |
| frontend-builder | *(none — apt not invoked)* | – |
| runtime | `libssl3 ca-certificates` | OpenSSL shared library; system CA bundle |

### Key dependency analysis from `Cargo.toml` and build artifacts

- **`reqwest 0.13`** — configured with `features = ["json", "rustls", "rustls-native-certs"], default-features = false`. Uses **rustls** (pure-Rust TLS), **not** native-tls / openssl-sys directly.  
- **`aws-lc-rs` / `aws-lc-sys`** — build artifacts (`target/debug/build/aws-lc-rs-*`, `aws-lc-sys-*`) confirm these are compiled transitive dependencies. Pulled in because rustls ≥ 0.23 defaults to `aws-lc-rs` as its crypto provider. `aws-lc-sys` wraps AWS's BoringSSL fork (C + C++ source) and **requires cmake, a C++ compiler, and perl** at build time.  
- **`ring`** — also present in build artifacts; used by some other transitive dependency.  
- **`libsqlite3-sys` (bundled SQLite)** — build artifacts confirm bundled compilation. Requires a C compiler at build time but no system sqlite library at runtime.  
- **`zbus 5`** (pure-Rust D-Bus, `default-features = false, features = ["tokio"]`) — no C bindings.  
- **`bcrypt 0.19`** — pure-Rust implementation; no C bindings.  
- **SQLx usage** — project uses `sqlx::raw_sql()` with embedded SQL (not `sqlx::query!()` compile-time macros). **No `DATABASE_URL` env var or `.sqlx/` offline cache required at build time.**

---

## 2. Problem Definition

The current runtime image (`debian:bookworm-slim`) is ~80 MB. Alpine 3.21 is ~7 MB, yielding a significantly smaller final image. Alpine is also better suited for minimal-attack-surface production deployments.

Key concerns that must be resolved before migration:

1. **libc ABI mismatch** — glibc binaries do not run on Alpine (musl). Builders must also switch to Alpine/musl so the binary libc matches the runtime.  
2. **`aws-lc-sys` build requirements** — BoringSSL-derived C/C++ code needs `cmake`, `g++`, and `perl` during compilation; these are absent from the base `rust:1.85-alpine` image.  
3. **Bundled SQLite** — `libsqlite3-sys` compiles C from source; needs a working C compiler and musl headers.  
4. **Package name differences** — Alpine uses `pkgconf` instead of `pkg-config`; OpenSSL runtime package name differs.  
5. **CA certificates** — `rustls-native-certs` reads the system CA store at runtime; Alpine's `ca-certificates` package must be present in the final image.

---

## 3. Research Findings

### 3.1 libc compatibility

`rust:1.85-alpine` targets `x86_64-unknown-linux-musl` by default. Rust with musl produces a **fully statically linked** binary (musl is designed for static linking). This binary:

- Runs correctly on `alpine:3.21` (both musl) ✓  
- Does **not** run on `debian:bookworm-slim` (different libc) — but that is irrelevant here  
- Requires **no dynamic shared libraries** at runtime (libc, libssl, libsqlite are all statically linked in)

### 3.2 Backend builder — required apk packages

Replace `apt-get install -y pkg-config libssl-dev` with:

```
apk add --no-cache build-base cmake perl bash pkgconf openssl-dev
```

| Package | Replaces / Purpose |
|---------|-------------------|
| `build-base` | Meta-package: gcc, g++, make, binutils, musl-dev. Needed by `aws-lc-sys` (C++) and `libsqlite3-sys` (C). |
| `cmake` | Required by `aws-lc-sys` (BoringSSL uses CMake as its build system, minimum cmake 3.17). |
| `perl` | Required by some BoringSSL configuration scripts inside `aws-lc-sys`. |
| `bash` | BoringSSL build scripts use bash; Alpine's default shell is busybox ash. |
| `pkgconf` | Replaces `pkg-config` (Debian). Provides `pkg-config` shim for crates that use it (openssl-sys, etc.). |
| `openssl-dev` | Replaces `libssl-dev` (Debian). Provides OpenSSL headers + `.pc` files. Needed if any transitive crate resolves through `openssl-sys`. |

**Note on `openssl-sys`**: With `reqwest` using `rustls` exclusively (`default-features = false`), `openssl-sys` is not a direct dependency. However, `openssl-dev` + `pkgconf` are included defensively because:  
(a) transitive crates may probe for it; (b) some `openssl-sys` versions attempt `pkg-config` discovery and fail loudly without the `.pc` file even when not needed for linking.

**No additional env vars required**: With `pkgconf` installed, `openssl-sys`'s build script auto-discovers OpenSSL via `pkg-config`. `OPENSSL_DIR`, `OPENSSL_LIB_DIR`, etc. are not needed.

### 3.3 Frontend builder — required apk packages

`cargo install trunk` compiles Trunk from source as a **native binary**. Trunk depends on `reqwest`, which pulls in `aws-lc-rs` / `aws-lc-sys` on the same dependency chain as the backend. Therefore, the **same package set** is required as in the backend builder.

The WASM compilation itself (`wasm32-unknown-unknown` target) does not link against musl or require C build tools — the output is pure WebAssembly. However, the Trunk binary (compiled during `cargo install trunk`) does.

### 3.4 Runtime image — required apk packages

Replace `apt-get install -y libssl3 ca-certificates` with:

```
apk add --no-cache openssl ca-certificates
```

| Package | Purpose |
|---------|---------|
| `openssl` | On Alpine, `libssl.so.3` and `libcrypto.so.3` are part of the `openssl` package (not a separate `libssl3` sub-package as in Debian). Included defensively; the fully-static musl binary may not require it, but having it costs ≈ 2 MB and avoids runtime surprises. |
| `ca-certificates` | Provides `/etc/ssl/certs/ca-certificates.crt`. Required at runtime by `rustls-native-certs` (reads system trust store) and by `reqwest` for outgoing HTTPS probe requests. |

### 3.5 Alpine version

Use `alpine:3.21`. This is the current stable release (as of May 2026) and is the Alpine version used as the base of `rust:1.85-alpine`. Using the same Alpine version for the runtime image ensures musl version consistency.

### 3.6 SQLx build-time requirements

The project uses `sqlx::raw_sql()` (runtime-only SQL execution) — **not** `sqlx::query!()` compile-time macros. Therefore:

- No `DATABASE_URL` environment variable is needed during `docker build`  
- No `SQLX_OFFLINE=true` flag is needed  
- No `.sqlx/` query cache directory is needed

---

## 4. Proposed Solution — Exact Dockerfile

```dockerfile
# Stage 1: Build Rust backend
FROM rust:1.85-alpine AS backend-builder
RUN apk add --no-cache \
    build-base \
    cmake \
    perl \
    bash \
    pkgconf \
    openssl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release --bin vexboard-server

# Stage 2: Build frontend (Trunk + WASM)
FROM rust:1.85-alpine AS frontend-builder
RUN apk add --no-cache \
    build-base \
    cmake \
    perl \
    bash \
    pkgconf \
    openssl-dev
RUN rustup target add wasm32-unknown-unknown && \
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

### Change summary vs. current Dockerfile

| Stage | Old | New |
|-------|-----|-----|
| backend-builder base | `rust:1.85-slim` | `rust:1.85-alpine` |
| backend-builder packages | `apt-get: pkg-config libssl-dev` | `apk: build-base cmake perl bash pkgconf openssl-dev` |
| frontend-builder base | `rust:1.85-slim` | `rust:1.85-alpine` |
| frontend-builder packages | *(none)* | `apk: build-base cmake perl bash pkgconf openssl-dev` |
| runtime base | `debian:bookworm-slim` | `alpine:3.21` |
| runtime packages | `apt-get: libssl3 ca-certificates` | `apk: openssl ca-certificates` |

No changes to `COPY`, `WORKDIR`, `EXPOSE`, or `ENTRYPOINT` directives — these are identical.

---

## 5. Implementation Steps

1. Replace the contents of `Dockerfile` with the new Dockerfile shown in Section 4.
2. No changes required to `Cargo.toml`, `Cargo.lock`, `docker-compose.yml`, source code, or configuration files.
3. Verify `docker-compose.yml` — the build context, exposed port (7280), volumes (`/var/lib/vexboard`, `/run/dbus/system_bus_socket`), and environment variables remain unchanged and fully compatible.

---

## 6. Risks and Mitigations

### Risk 1: `aws-lc-sys` build failure on musl (HIGH)

**Description**: `aws-lc-sys` compiles BoringSSL (C + C++). Known incompatibilities exist between BoringSSL and musl libc in some versions, including:  
- Missing `pthread_attr_getstack` in musl (used by some BoringSSL internal threading code)  
- Differences in `errno` handling and POSIX extensions  
- cmake version minimums not met

**Mitigation**:  
- `build-base`, `cmake`, `perl`, `bash` package additions address the toolchain gap  
- If the build fails with musl-specific errors from aws-lc-sys, add the following env var to the backend-builder stage to force aws-lc-sys into a compatibility mode or switch to the `ring` crypto provider:

  ```dockerfile
  ENV CARGO_FEATURE_RING=1
  ```

  Alternatively, pin `rustls` to use `ring` as the crypto backend by adding to `Cargo.toml`:
  ```toml
  [dependencies]
  rustls = { version = "0.23", default-features = false, features = ["ring"] }
  ```
  This avoids the aws-lc-sys C++ build entirely (ring is pure Rust + assembly).

### Risk 2: Runtime `libgcc` / `libstdc++` dependency (LOW)

**Description**: If `aws-lc-sys`'s C++ code is compiled with a shared-library link to `libstdc++`, the runtime image will need `libstdc++` (`apk add libstdc++`). With musl's fully-static default this is unlikely but possible with g++ on Alpine.

**Mitigation**: If the binary fails at runtime with `Error loading shared library libstdc++.so.6`, add `libstdc++` to the runtime `apk add` line.

### Risk 3: `cargo install trunk` build time (MEDIUM)

**Description**: Compiling Trunk from source in the frontend-builder stage can take 10–20 minutes. This is unchanged from the current Debian-based build.

**Mitigation**: Use Docker layer caching. The `RUN apk add ... && rustup target add ... && cargo install trunk` step should be placed before `COPY` so it is cached across source-only changes. The current Dockerfile already benefits from this ordering.

### Risk 4: Trunk downloads wasm-bindgen-cli at build time (LOW)

**Description**: Trunk downloads `wasm-bindgen-cli` during `trunk build --release`. This requires network access during the Docker build and the downloaded binary must be compatible with Alpine musl.

**Mitigation**: Trunk downloads pre-built `wasm-bindgen-cli` binaries for `x86_64-unknown-linux-musl` when running on Alpine. This is a supported target. No action needed unless building in a network-restricted environment (in that case, pre-install `wasm-bindgen-cli` via `cargo install wasm-bindgen-cli` before the `trunk build` step).

### Risk 5: musl stack size differences (LOW)

**Description**: musl has a smaller default thread stack size (128 KB) compared to glibc (8 MB). Deep async call stacks in Tokio tasks could in theory overflow.

**Mitigation**: Tokio's default stack size is configured independently; standard async/await patterns in Axum applications do not typically exhaust 128 KB stacks. Monitor for `SIGSEGV` on thread boundaries; if observed, set `RUST_MIN_STACK` environment variable.

### Risk 6: D-Bus socket on Alpine (INFO)

**Description**: `zbus` connects to `/run/dbus/system_bus_socket`. This requires the socket to be bind-mounted at container start (configured in `docker-compose.yml`, unchanged).

**Mitigation**: No change required. The `docker-compose.yml` volume mount `- /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro` is Alpine-compatible; socket paths are the same on both Debian and Alpine host systems.

---

## 7. Dependencies

No new Rust crate dependencies are introduced. No `Cargo.toml` changes required. The migration is purely a Dockerfile change.

**Build tool dependencies (Docker builder stages only)**:

| Package | Version on Alpine 3.21 | Role |
|---------|----------------------|------|
| `build-base` | ≥ 0.5 (meta) | C/C++ compiler suite |
| `cmake` | ≥ 3.28 | aws-lc-sys build system |
| `perl` | ≥ 5.38 | BoringSSL configure scripts |
| `bash` | ≥ 5.2 | BoringSSL build scripts |
| `pkgconf` | ≥ 2.1 | pkg-config shim |
| `openssl-dev` | ≥ 3.3 | OpenSSL headers / .pc file |

---

## 8. Spec File Path

`c:\Projects\vexboard\.github\docs\subagent_docs\alpine_dockerfile_spec.md`
