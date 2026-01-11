# syntax=docker/dockerfile:1
# Build stage for operator-web WASM dashboard
FROM --platform=$BUILDPLATFORM rust:1.75-slim AS builder

# Installwasm-pack and build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    pkg-config \
    libssl-dev \
    binaryen \
    && rm -rf /var/lib/apt/lists/*

# Install wasm-pack
RUN cargo install wasm-pack --locked

# Build arguments
ARG TARGETARCH
ARG VERSION=latest

# Build the WASM dashboard
WORKDIR /app
COPY . .
RUN wasm-pack build \
    --target web \
    --out-dir dist \
    --release \
    -- \
    -p hodei-console-web

# Production stage - create minimal image with static files
FROM scratch

# Labels
LABEL maintainer="Hodei Team <team@hodei.io>"
LABEL org.opencontainers.image.source="https://github.com/rubentxu/hodei-jobs"
LABEL org.opencontainers.image.description="Hodei Console Web - Leptos WASM"
LABEL version="${VERSION}"

# Copy WASM artifacts
COPY --from=builder /app/crates/console-web/dist /www

# Set working directory
WORKDIR /www

# Expose web port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

# Entrypoint for static file serving
# This image is designed to be used with a reverse proxy or as a sidecar
# Use with: docker run -p 8080:8080 -v /www:/www hodei/console-web

# Default command (placeholder - use a static file server)
CMD ["sh", "-c", "echo 'Use a static file server like caddy or nginx' && sleep infinity"]
