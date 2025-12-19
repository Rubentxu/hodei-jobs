# Master Plan: Worker Agent Improvements & Alignment Report

**Date:** 2025-12-19
**Status:** Final Analysis
**Version:** 1.0

## 1. Executive Summary

This document serves as the comprehensive "Master Plan" for the Hodei Job Platform Worker Agent. It consolidates all requirements from `PRD-V7.0`, `architecture.md`, and alignment documents into a single specification, and compares them against the current codebase (`v8.0` candidate).

**Current Status Alignment:** 90% Aligned.
**Major Gaps:** Graceful Shutdown, Batching Configuration.

---

## 2. Consolidated Specification (The "North Star")

The Worker Agent is designed as a robust, resilient, and observable executable responsible for running jobs in distributed environments.

### 2.1 Core Architecture

- **Language**: Rust (Edition 2021+).
- **Runtime**: `tokio` (Async/Await).
- **Communication**: gRPC (via `tonic` / `connect-rpc`) with bidirectional streaming.
- **Design Pattern**: Hexagonal / Domain-Driven Design (DDD).
  - **Domain**: Job logic, resource constraints.
  - **Infrastructure**: gRPC adapters, File system, Process execution.

### 2.2 Functional Requirements

#### A. Observability & Logging (High Priority)

- **Structured Logging**: All internal logs must use structured formats (JSON/Key-Value) via `tracing`.
- **Job Output Capture**: Stdout and Stderr of executed jobs must be captured independently.
- **Log Batching**: To reduce network overhead, logs must be buffered and sent in batches, not individually.
- **Backpressure**: The worker must drop logs if the network channel is full, prioritizing job execution integrity over log completeness during congestion.

#### B. Resource Management & Telemetry

- **Enriched Heartbeat**: Periodic heartbeats must include real-time metrics:
  - CPU Usage (%)
  - Memory Usage (Used/Available)
  - Disk Usage (Available space)
- **Adaptive Behavior**: Worker should report "Busy" or high resource usage to influence scheduler decisions.

#### C. Resilience & Reliability

- **Connection Resilience**:
  - Exponential Backoff for reconnections.
  - Auto-reconnect on stream drop.
  - HTTP/2 Keep-Alive.
- **Job Safety**:
  - **Timeouts**: Strict enforcement of execution time limits.
  - **Isolation**: Process-level isolation at minimum.
  - **Cancellation**: Ability to abort running jobs upon server request.

#### D. Security

- **Mutual TLS (mTLS)**: Support for client-side certificates for authentication and encryption.
- **Token Auth**: Fallback/Bootstrap using OTP/Auth Tokens.
- **Certificate Management**: (Phase 4) Automatic rotation and validation.

---

## 3. Code Alignment Analysis

We have performed a deep-dive review of the codebase (specifically `crates/worker`) against the specifications above.

### ✅ Fully Aligned Features

1.  **Architecture**: The project organization follows strict separation of concerns:

    - `crates/worker/bin`: Entry point.
    - `crates/worker/infrastructure`: Adapter implementations (`JobExecutor`, `MetricsCollector`).
    - `crates/worker/domain`: Core logic.

2.  **Observability**:

    - **Tracing**: Implemented in `main.rs` using `tracing_subscriber`.
    - **Std/Stderr Capture**: `JobExecutor` uses `FramedRead` to capture output streams separately.
    - **Backpressure**: `LogBatcher` in `logging.rs` explicitly handles channel saturation:
      ```rust
      // logging.rs:126
      warn!("Log batch dropped due to backpressure");
      ```

3.  **Resource Telemetry**:

    - `MetricsCollector` (in `metrics.rs`) utilizes `sysinfo` to capture CPU and RAM.
    - This data is correctly mapped to `WorkerHeartbeat` messages in the main loop.

4.  **Resilience**:

    - **Reconnection**: `main.rs` contains a `loop` with `backoff` logic (exponential increase up to 60s).
    - **Timeouts**: `JobExecutor` wraps execution in `tokio::time::timeout`.
    - **Cancellation**: `ServerPayload::Cancel` is handled by aborting the `JoinHandle`.

5.  **Security**:
    - **mTLS**: `main.rs` includes logic to load `client_cert`, `client_key`, and `ca_cert` if enabled in config.

### ⚠️ Partial / Misaligned Features

1.  **Log Batching Configuration**:

    - **Status**: Technically implemented.
    - **Issue**: The constants in `logging.rs` effectively disable it:
      ```rust
      pub const LOG_BATCHER_CAPACITY: usize = 1; // Should be higher (e.g., 100)
      pub const LOG_FLUSH_INTERVAL_MS: u64 = 10; // Should be higher (e.g., 500ms)
      ```
    - **Impact**: Logs are streamed almost one-by-one, negating the performance benefit.

2.  **Graceful Shutdown**:
    - **Status**: **MISSING**.
    - **Issue**: The `main.rs` loop does not listen for `SIGINT` (Ctrl+C) or `SIGTERM`.
    - **Impact**: If the worker process is stopped, it might kill running jobs abruptly without sending a "Going Away" status or flushing pending logs.

---

## 4. Integration Plan & Timeline

Based on the alignment check, here is the immediate action plan:

### Step 1: Fix Graceful Shutdown (High Priority)

- **Action**: Integrate `tokio::signal::ctrl_c()` into the `tokio::select!` loop in `main.rs`.
- **Behavior**: On signal, stop accepting new jobs, wait (with timeout) for running jobs or cancel them gracefully, flush logs, and exit.

### Step 2: Tune Batching (Medium Priority)

- **Action**: Update `LOG_BATCHER_CAPACITY` to `100` and `LOG_FLUSH_INTERVAL_MS` to `250` in `logging.rs` (or move to Config).

### Step 3: Implement Certificate Rotation (Future - Phase 4)

- **Action**: Implement the `initiate_renewal` method in `tls.rs`.

---

## 5. Conclusion

The Worker Agent codebase is **highly mature and well-aligned** with the V8 architecture goals. The core complex mechanics (Backpressure, mTLS, Telemetry) are already in place and correct. The only remaining steps are configuration tuning and adding standard lifecycle management (Shutdown).
