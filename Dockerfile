# =============================================================================
# Hodei Jobs Platform - Production Dockerfile
# =============================================================================
# Build stage - using latest to get the most current Rust version
FROM rust:latest AS builder

WORKDIR /app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY proto/ proto/
COPY crates/ crates/

# Build release (single command to leverage cargo's incremental compilation)
RUN cargo build --release -p hodei-jobs-cli -p hodei-jobs-grpc

# Runtime stage - using stable-slim to match the GLIBC version of rust:latest
FROM debian:stable-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries
COPY --from=builder /app/target/release/hodei-jobs-cli /usr/local/bin/
COPY --from=builder /app/target/release/server /usr/local/bin/hodei-jobs-server

# Default command
CMD ["hodei-jobs-server"]
