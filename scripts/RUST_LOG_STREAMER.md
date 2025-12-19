# Rust Log Streamer - Real-time Job Log Display

## Overview

This Rust-based gRPC client provides **true real-time log streaming** for Hodei Job Platform, replacing the Python/grpcurl solution that buffered output.

## Problem Solved

Previously, logs were displayed all at once when the job finished, not progressively as generated. This was due to:
1. **grpcurl buffering**: grpcurl internally buffers gRPC stream output
2. **Python buffering**: The Python script also added buffering layers
3. **Bash buffering**: Background processes in bash can buffer output

## Solution: Rust Client with Zero Buffering

### Key Features
- ✅ **Built with Tonic**: Native Rust gRPC client using the same framework as the server
- ✅ **stderr output**: Uses stderr which is unbuffered by default
- ✅ **No intermediate buffering**: Direct connection to gRPC stream
- ✅ **Immediate flush**: Each log line is displayed instantly
- ✅ **Formatted output**: Millisecond timestamps with color-coded prefixes

### Implementation Details

#### Worker Configuration (Real-time)
```rust
// crates/worker/infrastructure/src/logging.rs
pub const LOG_BATCHER_CAPACITY: usize = 1;           // No batching
pub const LOG_FLUSH_INTERVAL_MS: u64 = 10;           // 10ms flush
```

#### Server Configuration (Immediate Forwarding)
```rust
// crates/server/interface/src/grpc/log_stream.rs
// Logs are immediately forwarded to all subscribers
```

#### Client Configuration (Zero Buffering)
```rust
// scripts/stream-logs-rust/src/main.rs
// All output goes to stderr (unbuffered)
eprintln!("{} {} {}", ts_str, prefix, log_entry.line);
```

## Usage

### Direct Usage
```bash
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs <job-id>
```

### Via Job Scripts

All job scripts automatically use the Rust client:

1. **maven_job_with_logs.sh**
   - Background execution with `stdbuf -o0 -e0 ... | cat &`
   - Includes auto-build logic

2. **trace-job.sh**
   - Background execution with `stdbuf -o0 -e0 ... | cat &`
   - Monitors job while streaming

3. **data_processing_simple.sh**
   - Foreground execution (immediate output)
   - Auto-build on first use

4. **cicd_simple.sh**
   - Foreground execution
   - Auto-build on first use

5. **cicd_pipeline.sh**
   - Foreground execution
   - Auto-build on first use

6. **data_processing_pipeline.sh**
   - Foreground execution
   - Auto-build on first use

## Build & Installation

The client is built automatically by job scripts if not present:

```bash
# Manual build
cd /home/rubentxu/Proyectos/rust/package/hodei-job-platform
cargo build --release --bin stream-logs

# Create symlink
ln -sf /home/rubentxu/Proyectos/rust/package/hodei-job-platform/target/release/stream-logs \
       /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs
```

## Output Format

```
📡 Streaming logs for job: 550e8400-e29b-41d4-a716-446655440000
────────────────────────────────────────────────────────────────────────────
14:23:45.123 [OUT] Starting ETL Pipeline...
14:23:47.234 [OUT] [1/3] Generating data...
14:23:49.345 [OUT] Generated 100K customer records
14:23:51.456 [OUT] [2/3] Transforming data...
14:23:53.567 [OUT] Data transformed successfully
14:23:55.678 [OUT] [3/3] Loading to database...
14:23:57.789 [OUT] Data loaded successfully
14:23:59.890 [OUT] ETL Pipeline completed!
14:24:00.001 [OUT] ✅ Job completed successfully

❌ Stream ended
```

- **Timestamps**: Millisecond precision (HH:MM:SS.mmm)
- **[OUT]**: Standard output (stdout)
- **[ERR]**: Error output (stderr)
- **Real-time**: Logs appear as they're generated, not at job completion

## Technical Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Worker Process                                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ LogBatcher (capacity=1, flush=10ms)                 │  │
│  │  → Sends each log line immediately                   │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            ↓ gRPC Stream
┌─────────────────────────────────────────────────────────────┐
│ Server LogStreamService                                     │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Forwards logs to all subscribers                     │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            ↓ gRPC Stream
┌─────────────────────────────────────────────────────────────┐
│ Rust Client (stream-logs)                                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ → Receives log                                       │  │
│  │ → Formats with timestamp                             │  │
│  │ → Outputs to stderr (unbuffered)                     │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            ↓
                         Terminal (real-time display)
```

## Buffering Layers Eliminated

| Layer | Before | After |
|-------|--------|-------|
| Worker | Batch of 100, 100ms | Single line, 10ms |
| Server | Standard forwarding | Immediate forwarding |
| Client | Python/grpcurl | Rust stderr |
| Shell | Buffered background | stdbuf + foreground |
| Terminal | Line-buffered | Unbuffered stderr |

## Benefits

1. **True Real-time**: See logs as they're generated, not at completion
2. **Better Debugging**: Identify issues immediately during job execution
3. **No Dependencies**: No Python, grpc library, or jq required
4. **Faster**: Native Rust binary with optimized gRPC handling
5. **Consistent**: All scripts use the same client
6. **Automatic**: Builds itself if missing

## Testing

To verify real-time streaming:

```bash
# Run a simple job and watch logs appear progressively
cd /home/rubentxu/Proyectos/rust/package/hodei-job-platform
./scripts/Job_Execution/data_processing_simple.sh --etl

# You should see:
# - Logs appear one by one, not all at once
# - Each line appears ~2 seconds apart (as generated)
# - Not all lines appearing after job completes
```

## Troubleshooting

If logs still appear buffered:

1. **Check client exists**:
   ```bash
   ls -lah /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs
   ```

2. **Rebuild client**:
   ```bash
   cd /home/rubentxu/Proyectos/rust/package/hodei-job-platform
   cargo build --release --bin stream-logs
   ```

3. **Test manually**:
   ```bash
   /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs <job-id>
   ```

4. **Check worker config**:
   ```bash
   grep -A2 "LOG_BATCHER" crates/worker/infrastructure/src/logging.rs
   # Should show: capacity=1, flush=10ms
   ```

## Files

- **Source**: `scripts/stream-logs-rust/src/main.rs`
- **Binary**: `target/release/stream-logs`
- **Symlink**: `scripts/stream-logs`
- **Cargo.toml**: `scripts/stream-logs-rust/Cargo.toml`

## Future Enhancements

Potential improvements:
- Color-coded log levels
- Search/filter capabilities
- Export logs to file
- Timestamp synchronization
- Multiple job monitoring
