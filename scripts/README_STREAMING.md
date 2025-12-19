# Real-Time Log Streaming - Test Scripts

## Problem
Even with the Rust client, logs might not appear in real-time due to terminal/shell buffering.

## Solution: Multiple Test Methods

I've created several test scripts to diagnose and solve this:

### 1. Quick Test (5 seconds)
```bash
./scripts/test-stream.sh
```
- Submits a 5-second job
- Tests basic Rust client
- Shows if streaming works at all

### 2. Comprehensive Diagnostic (3 methods)
```bash
./scripts/diagnostic-stream.sh
```
- Test 1: Basic Rust client (stderr)
- Test 2: With `stdbuf` (line buffering)
- Test 3: Background with unbuffered pipe
- Compares all methods

### 3. Unbuffered Output Test
```bash
./scripts/test-unbuffered.sh
```
- Uses `script` command to force unbuffered output
- Bypasses all terminal buffering
- **Most reliable method**

### 4. Stress Test (45 seconds)
```bash
./scripts/Job_Execution/stress_test_logs.sh
```
- 30 iterations, 1.5s apart
- Perfect for verifying real-time streaming
- Shows clear progression of logs

### 5. Simple Client Test
```bash
./scripts/test-rust-client.sh
```
- Tests if Rust client can connect
- Verifies server is running
- Basic connectivity check

## How to Use

### Step 1: Test Basic Connectivity
```bash
./scripts/test-rust-client.sh
```
**Expected**: Client tries to connect (may timeout)

### Step 2: Quick Stream Test
```bash
./scripts/test-stream.sh
```
**Expected**: See 5 logs appear over 5 seconds

### Step 3: Try Unbuffered Method
```bash
./scripts/test-unbuffered.sh
```
**Expected**: Lines appear 1 second apart

### Step 4: Full Diagnostic
```bash
./scripts/diagnostic-stream.sh
```
**Expected**: See which method works best

### Step 5: Stress Test
```bash
./scripts/Job_Execution/stress_test_logs.sh
```
**Expected**: 30 logs appear over 45 seconds

## If Streaming Doesn't Work

### Method 1: Use `script` command
Modify your scripts to use:
```bash
script -q -c "/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID" /dev/null
```

### Method 2: Use `stdbuf`
```bash
stdbuf -oL -eL /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID
```

### Method 3: Use `unbuffer` (if available)
```bash
unbuffer -p /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID
```

## Updated Script Templates

### Foreground Execution (for scripts that run client directly):
```bash
# In your script, replace:
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID"

# With:
stdbuf -oL -eL /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID" 2>&1
```

### Background Execution (for scripts that monitor job):
```bash
# Replace:
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID" &

# With:
stdbuf -o0 -e0 /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID" 2>&1 | cat &
```

## Key Points

1. **Rust client outputs to stderr** (unbuffered by default)
2. **Worker sends logs every 10ms** (capacity=1)
3. **Server forwards immediately** to all subscribers
4. **Terminal/shell may buffer** output

## The Fix

Use `stdbuf` or `script` command to prevent buffering:
- `stdbuf -oL`: Line buffered output
- `stdbuf -o0`: Unbuffered output
- `script -q -c`: Forces unbuffered terminal I/O

## Quick Reference

| Scenario | Command |
|----------|---------|
| Simple test | `./scripts/test-stream.sh` |
| Unbuffered | `./scripts/test-unbuffered.sh` |
| Full diag | `./scripts/diagnostic-stream.sh` |
| Long test | `./scripts/Job_Execution/stress_test_logs.sh` |
| Background | `stdbuf -o0 -e0 script 2>&1 \| cat &` |
| Foreground | `stdbuf -oL -eL script 2>&1` |

## Expected Behavior

**CORRECT**: Logs appear progressively
```
14:23:45.123 [OUT] Line 1
14:23:46.234 [OUT] Line 2  ← 1.1s later
14:23:47.345 [OUT] Line 3  ← 1.1s later
```

**WRONG**: All logs appear at once
```
14:23:50.000 [OUT] Line 1
14:23:50.000 [OUT] Line 2
14:23:50.000 [OUT] Line 3  ← All at same time
```
