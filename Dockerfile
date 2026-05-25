# Stage 1: Build Rust backend
FROM docker.io/library/rust:1.88-alpine AS backend-builder
RUN apk add --no-cache build-base cmake perl bash pkgconf openssl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release --bin vexboard-server

# Stage 2: Build frontend (Trunk + WASM)
FROM docker.io/library/rust:1.88-alpine AS frontend-builder
RUN apk add --no-cache build-base cmake perl bash pkgconf openssl-dev && \
    rustup target add wasm32-unknown-unknown && \
    cargo install trunk
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
WORKDIR /build/crates/vexboard-frontend
RUN trunk build --release

# Stage 3: Runtime
FROM docker.io/library/alpine:3.21
RUN apk add --no-cache openssl ca-certificates
WORKDIR /app
COPY --from=backend-builder /build/target/release/vexboard-server ./vexboard
COPY --from=frontend-builder /build/crates/vexboard-frontend/dist ./assets
COPY config/ ./config/
RUN mkdir -p /var/lib/vexboard
EXPOSE 7280
ENTRYPOINT ["./vexboard"]
