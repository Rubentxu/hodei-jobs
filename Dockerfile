# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY proto/ proto/
COPY crates/ crates/

# Build release
RUN cargo build --release -p hodei-jobs-cli
RUN cargo build --release -p hodei-jobs-grpc --bin server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries
COPY --from=builder /app/target/release/hodei-jobs-cli /usr/local/bin/
COPY --from=builder /app/target/release/server /usr/local/bin/server

# Default command
CMD ["server"]
