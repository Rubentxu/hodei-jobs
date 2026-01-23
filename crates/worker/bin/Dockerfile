# Multi-stage build for Hodei Worker
#
# Build from project root: cd /path/to/hodei-jobs && podman build -f crates/worker/bin/Dockerfile -t ...
#
# Or build binary locally and use it directly:
# cargo build --release -p hodei-worker-bin
# podman build -f crates/worker/bin/Dockerfile --build-arg BINARY_PATH=./target/release/hodei-worker-bin -t ...
#
FROM docker.io/library/alpine:3.20 AS runner

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    curl \
    protobuf-dev

# Create a non-root user
RUN addgroup -g 1000 hodei && \
    adduser -D -u 1000 -G hodei hodei

# Set working directory
WORKDIR /app

# Copy the worker binary (passed as build arg or built in builder stage)
ARG BINARY_PATH
COPY ${BINARY_PATH:-./target/release/hodei-worker-bin} /app/hodei-worker-bin

# Set permissions
RUN chown -R hodei:hodei /app/hodei-worker-bin && \
    chmod +x /app/hodei-worker-bin

# Switch to non-root user
USER hodei

# Set the entrypoint
ENTRYPOINT ["/app/hodei-worker-bin"]
