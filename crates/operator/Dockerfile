# Multi-stage build for Hodei Operator
# Optimized for small image size and security

# =============================================================================
# STAGE 1: Builder
# =============================================================================
FROM rust:1-alpine AS builder

WORKDIR /build

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static protoc protobuf-dev

# Copy workspace files first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates/operator/Cargo.toml ./crates/operator/Cargo.toml

# Copy proto files and build script
COPY proto/ ./proto/
COPY proto/build.rs ./proto/build.rs

# Build dependencies (cached)
RUN cargo fetch 2>/dev/null || true

# Copy full source and build
COPY . .
RUN cargo build -p hodei-operator --bin hodei-operator --release --features dashboard

# =============================================================================
# STAGE 2: WASM Assets (pre-built locally)
# =============================================================================
FROM scratch AS wasm-assets

# Copy pre-built WASM assets from local build
COPY operator-web-dist/ /var/www/html/

# =============================================================================
# STAGE 3: Runtime - Distroless image
# =============================================================================
FROM gcr.io/distroless/cc:nonroot AS runtime

# Add required certificates for TLS
FROM debian:bookworm-slim AS certs
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Final distroless image with certs
FROM gcr.io/distroless/cc:latest

# Copy certificates from debian stage
COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the operator binary
COPY --from=builder /build/target/release/hodei-operator /hodei-operator

# Copy WASM assets
COPY --from=wasm-assets /var/www/html /var/www/html

# Use non-root user (default in distroless)
USER 1000:1000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ["/hodei-operator", "health"] || exit 1

ENTRYPOINT ["/hodei-operator"]
