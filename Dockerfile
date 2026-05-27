# ── Stage 0: Base with cargo-chef (cached until Rust version changes) ──────────
FROM docker.io/library/rust:1.88-alpine AS chef
RUN apk add --no-cache build-base cmake perl bash pkgconf openssl-dev curl
RUN cargo install cargo-chef --locked
WORKDIR /build

# ── Stage 1: Generate dependency recipe from Cargo manifests ──────────────────
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: Build Rust backend ───────────────────────────────────────────────
FROM chef AS backend-builder
COPY --from=planner /build/recipe.json recipe.json
# Pre-build all dependencies — this layer is cached until Cargo.lock changes
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release --bin vexboard-server

# ── Stage 3: Build frontend (Trunk + WASM) ────────────────────────────────────
FROM chef AS frontend-builder
RUN rustup target add wasm32-unknown-unknown
# Use pre-built Trunk binary — avoids compiling ~400 crates from source
RUN curl -sSfL \
    https://github.com/trunk-rs/trunk/releases/latest/download/trunk-x86_64-unknown-linux-musl.tar.gz \
    | tar -xz -C /usr/local/bin trunk
COPY --from=planner /build/recipe.json recipe.json
# Pre-build WASM dependencies — scoped to frontend only to avoid server-side
# native deps (mio/tokio-net) that cannot compile for wasm32-unknown-unknown
RUN cargo chef cook --release --target wasm32-unknown-unknown --package vexboard-frontend --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
WORKDIR /build/crates/vexboard-frontend
RUN trunk build --release

# ── Stage 4: Runtime ──────────────────────────────────────────────────────────
FROM docker.io/library/alpine:3.21
RUN apk add --no-cache openssl ca-certificates
WORKDIR /app
COPY --from=backend-builder /build/target/release/vexboard-server ./vexboard
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
COPY config/ ./config/
RUN mkdir -p /var/lib/vexboard
EXPOSE 7280
ENTRYPOINT ["./vexboard"]
