# Rust Job Runner - Complete Solution

## Overview

The Rust Job Runner (`stream-logs`) is a comprehensive tool that:
1. **Creates and submits jobs** to the Hodei platform
2. **Subscribes to log streams** in real-time
3. **Displays logs with timestamps** as they arrive
4. **Handles both commands and script files**

## Usage

### Run with Cargo

```bash
cargo run --bin stream-logs -- [OPTIONS]
```

### Options

| Option | Description | Example |
|--------|-------------|---------|
| `--name <name>` | Job name | `--name "My-Job"` |
| `--command <cmd>` | Command to execute | `--command "echo hello"` |
| `--args <args>` | Command arguments (space-separated) | `--args "-c 'echo test'"` |
| `--script <file>` | Path to script file | `--script /path/to/script.sh` |
| `--cpu <cores>` | CPU cores (default: 1.0) | `--cpu 2.0` |
| `--memory <bytes>` | Memory in bytes (default: 1073741824) | `--memory 2147483648` |
| `--timeout <duration>` | Timeout (default: 600s) | `--timeout 900s` |
| `--output <file>` | Write logs to file | `--output /tmp/logs.txt` |
| `--job-id <id>` | Subscribe to existing job | `--job-id 12345-abc...` |

## Examples

### 1. Simple Command

```bash
cargo run --bin stream-logs -- \
    --name "Simple-Test" \
    --command "echo 'Hello World'; sleep 1; echo 'Done'"
```

### 2. Script File

```bash
# Create a script
cat > /tmp/my-script.sh << 'EOF'
#!/bin/bash
echo "Starting job..."
for i in {1..5}; do
    echo "Step $i"
    sleep 1
done
echo "Job complete!"
EOF

# Run it
cargo run --bin stream-logs -- \
    --name "Script-Test" \
    --script /tmp/my-script.sh
```

### 3. Complex Command with Arguments

```bash
cargo run --bin stream-logs -- \
    --name "Complex-Command" \
    --command "/bin/bash" \
    --args "-c 'for i in {1..10}; do echo Iteration \$i; sleep 0.5; done'"
```

### 4. High-Resource Job

```bash
cargo run --bin stream-logs -- \
    --name "Heavy-Computation" \
    --command "python3 -c 'import time; [time.sleep(0.1) for _ in range(100)]'" \
    --cpu 4.0 \
    --memory 4294967296 \
    --timeout 1800s
```

### 5. Subscribe to Existing Job

```bash
# If you have a running job
cargo run --bin stream-logs -- \
    --job-id "550e8400-e29b-41d4-a716-446655440000"
```

### 6. Write Logs to File

```bash
cargo run --bin stream-logs -- \
    --name "File-Output-Test" \
    --command "echo 'Line 1'; sleep 1; echo 'Line 2'" \
    --output /tmp/job-logs.txt
```

## Features

### Real-Time Log Streaming
- ✅ Logs appear as they're generated
- ✅ Millisecond timestamps
- ✅ Color-coded output ([OUT] for stdout, [ERR] for stderr)
- ✅ File-based streaming (no buffering issues)

### Job Management
- ✅ Automatic job creation and submission
- ✅ Job ID detection and extraction
- ✅ Status monitoring
- ✅ Resource allocation (CPU, memory, timeout)

### Flexible Execution
- ✅ Direct command execution
- ✅ Script file execution
- ✅ Argument handling
- ✅ Environment customization

## Output Format

```
╔════════════════════════════════════════════════════════════════╗
║         HODEI JOB EXECUTOR WITH LIVE LOG STREAMING            ║
╚════════════════════════════════════════════════════════════════╝

🔌 Connecting to gRPC server at localhost:50051...
✅ Connected!

📤 Submitting job: Test-Job
   Command: echo 'Line 1'; sleep 1; echo 'Line 2'
   CPU: 1 cores
   Memory: 1073741824 bytes
   Timeout: 600s

✅ Job queued successfully!

⏳ Waiting for job to be registered...
   Attempt 1/10...
✅ Found job!
   Job ID: 550e8400-e29b-41d4-a716-446655440000
   Status: JOB_STATUS_RUNNING

📡 Subscribing to log stream...
────────────────────────────────────────────────────────────

🚀 Streaming logs in real-time...
   Job ID: 550e8400-e29b-41d4-a716-446655440000
   Press Ctrl+C to stop watching

14:23:45.123 [OUT] Line 1
14:23:46.234 [OUT] Line 2

────────────────────────────────────────────────────────────

╔════════════════════════════════════════════════════════════════╗
║                    STREAM FINISHED                             ║
╚════════════════════════════════════════════════════════════════╝

📊 Summary:
   Job Name: Test-Job
   Job ID: 550e8400-e29b-41d4-a716-446655440000
   Logs Received: 2
   Duration: 1.234s

🔍 Getting final job status...
   Name: Test-Job
   Status: JOB_STATUS_COMPLETED

✅ Execution complete!
```

## Test Scripts

### Quick Test
```bash
./scripts/rust-job-runner.sh
```

### Individual Tests
```bash
# Test 1: Simple command
cargo run --bin stream-logs -- --name "Test-1" --command "echo test"

# Test 2: Script file
cargo run --bin stream-logs -- --name "Test-2" --script /path/to/script.sh

# Test 3: Long running job
cargo run --bin stream-logs -- --name "Test-3" --command "for i in {1..10}; do echo \$i; sleep 1; done"
```

## Battery of Test Jobs

### ETL Pipeline Test
```bash
cargo run --bin stream-logs -- \
    --name "ETL-Pipeline-Test" \
    --command "echo 'Starting ETL...'; echo '[1/3] Extracting data'; sleep 1; echo '[2/3] Transforming data'; sleep 1; echo '[3/3] Loading data'; sleep 1; echo 'ETL Complete!'"
```

### CI/CD Build Test
```bash
cargo run --bin stream-logs -- \
    --name "CI-CD-Build-Test" \
    --command "echo 'Cloning repo...'; sleep 1; echo 'Running tests...'; sleep 2; echo 'Building...'; sleep 2; echo 'Build complete!'"
```

### Data Processing Test
```bash
cargo run --bin stream-logs -- \
    --name "Data-Processing-Test" \
    --command "for i in {1..20}; do echo \"Processing record \$i/20\"; sleep 0.5; done; echo 'All records processed!'"
```

### Error Handling Test
```bash
cargo run --bin stream-logs -- \
    --name "Error-Handling-Test" \
    --command "echo 'Starting...'; echo 'Processing...'; ls /nonexistent 2>&1 || echo 'Error caught'; echo 'Continuing...'; echo 'Done'"
```

## Integration with Bash Scripts

Instead of using shell scripts with grpcurl, you can now use the Rust runner:

### Old Way (Bash + grpcurl)
```bash
# Bash script that submits job and uses grpcurl
RESPONSE=$(grpcurl -plaintext -d '{"job_definition": {...}}' localhost:50051 hodei.JobExecutionService/QueueJob)
JOB_ID=$(echo "$RESPONSE" | jq -r '.job_id')
grpcurl -plaintext -d "{\"job_id\": \"$JOB_ID\"}" localhost:50051 hodei.LogStreamService/SubscribeLogs
```

### New Way (Rust)
```bash
# Single command with the Rust runner
cargo run --bin stream-logs -- --name "My-Job" --command "my-command"
```

## Benefits

1. ✅ **Single Binary**: No need for grpcurl, jq, or bash wrappers
2. ✅ **Real-Time Streaming**: File-based output guarantees no buffering
3. ✅ **Type Safety**: Rust ensures correct API usage
4. ✅ **Automatic Job Management**: Creates job, gets ID, subscribes to logs
5. ✅ **Rich Output**: Formatted timestamps, status info, summaries
6. ✅ **Flexible**: Supports commands, scripts, and various options

## Troubleshooting

### Connection Error
```
❌ Failed to connect to server
```
**Solution**: Ensure server is running on localhost:50051

### Job Not Found
```
⚠️  Could not find job execution
```
**Solution**: Job might still be queuing. Wait a few seconds and try again.

### Stream Error
```
❌ Stream error: transport error
```
**Solution**: Check if job completed or was cancelled.

## Files

- **Source**: `scripts/stream-logs-rust/src/main.rs`
- **Binary**: `target/release/stream-logs`
- **Symlink**: `scripts/stream-logs`
- **Test Runner**: `scripts/rust-job-runner.sh`
- **Documentation**: `scripts/RUST_RUNNER.md`
