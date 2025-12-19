# =============================================================================
# Hodei Jobs Platform - Server Dockerfile
# =============================================================================
FROM rust:latest AS builder

WORKDIR /app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY proto/ proto/
COPY crates/ crates/

# Build only the server
ENV SQLX_OFFLINE=true
RUN cargo build --release -p hodei-server-bin

# Runtime stage
FROM debian:stable-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/hodei-server-bin /usr/local/bin/hodei-jobs-server

CMD ["hodei-jobs-server"]
