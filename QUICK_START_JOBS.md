# 🚀 Quick Start - Running Jobs (kubectl-style)

This guide shows how to run jobs and watch logs in real-time, similar to `kubectl`.

## Prerequisites

```bash
# Start the development environment
just dev

# Wait for server to be ready (you'll see the server logs)
```

## Running Example Jobs

### 1. Simple Hello World Job
```bash
just job-hello-world
```
**What it does:** Basic job with clear output showing initialization, processing, and completion.

### 2. Data Processing (ETL Pipeline)
```bash
just job-data-processing
```
**What it does:** Simulates:
- Data ingestion (10,000 records)
- Transformation in batches
- Validation
- Output generation (CSV, JSON, Parquet)

### 3. Machine Learning Training
```bash
just job-ml-training
```
**What it does:** Simulates:
- Environment setup
- Model initialization
- 10 epochs of training
- Validation and testing
- Model export

### 4. CI/CD Build Pipeline
```bash
just job-cicd-build
```
**What it does:** Simulates:
- Code checkout
- Dependency installation
- Linting and testing
- Building artifacts
- Uploading to registry

### 5. CPU Stress Test
```bash
just job-cpu-stress
```
**What it does:**
- CPU stress test (30 seconds)
- Memory allocation (1 GB)
- I/O operations (500 MB)

### 6. Error Handling Demo
```bash
just job-error-handling
```
**What it does:**
- Shows successful operations
- Demonstrates recoverable errors (with retry)
- Shows non-recoverable errors (with cleanup)
- **Exits with error code 42**

### Run All Examples
```bash
just job-examples-all
```
Runs all example jobs sequentially.

## Using hodei-cli Directly

### Run a Job and Stream Logs (like `kubectl apply && kubectl logs -f`)

```bash
# Run a simple command
cargo run --bin hodei-jobs-cli -- job run --name "My Job" --command "echo 'Hello from Hodei!'"

# Run a script file
cargo run --bin hodei-jobs-cli -- job run --name "Script Job" --script scripts/examples/01-hello-world.sh

# Run with custom resources
cargo run --bin hodei-jobs-cli -- job run --name "Heavy Job" --command "sleep 30" --cpu 2.0 --memory 2147483648
```

### Watch Logs in Real-Time (like `kubectl logs -f`)

```bash
# First, list jobs to get the job ID
cargo run --bin hodei-jobs-cli -- job list

# Then watch logs for a specific job
cargo run --bin hodei-jobs-cli -- logs follow --job-id <JOB_ID>
```

### Complete Workflow Example

```bash
# 1. Run a job and get its ID
JOB_OUTPUT=$(cargo run --bin hodei-jobs-cli -- job run --name "Demo Job" --script scripts/examples/01-hello-world.sh)
# The job ID will be in the output

# 2. Watch logs in another terminal
cargo run --bin hodei-jobs-cli -- logs follow --job-id <JOB_ID>

# 3. Check job status
cargo run --bin hodei-jobs-cli -- job get --job-id <JOB_ID>

# 4. List all jobs
cargo run --bin hodei-jobs-cli -- job list
```

## Job Commands Reference

### Create/Run Jobs
```bash
# Run job with command (immediate execution + log streaming)
cargo run --bin hodei-jobs-cli -- job run --name "Job Name" --command "echo 'Hello'"

# Run job with script file
cargo run --bin hodei-jobs-cli -- job run --name "Job Name" --script /path/to/script.sh

# Queue job (no streaming, just queue)
cargo run --bin hodei-jobs-cli -- job queue --name "Job Name" --command "echo 'Hello'"
```

### Monitor Jobs
```bash
# List all jobs
cargo run --bin hodei-jobs-cli -- job list

# Get job details
cargo run --bin hodei-jobs-cli -- job get --job-id <JOB_ID>

# Cancel a job
cargo run --bin hodei-jobs-cli -- job cancel --job-id <JOB_ID>

# Watch logs in real-time
cargo run --bin hodei-jobs-cli -- logs follow --job-id <JOB_ID>

# Get historical logs
cargo run --bin hodei-jobs-cli -- logs get --job-id <JOB_ID>
```

## Creating Your Own Jobs

### Option 1: Use `--command`
```bash
cargo run --bin hodei-jobs-cli -- job run --name "My Job" --command "echo 'Hello'; sleep 5; echo 'Done'"
```

### Option 2: Create a Script File
```bash
# Create your script
cat > /tmp/my-job.sh << 'EOF'
#!/bin/bash
echo "Starting my job..."
echo "Processing data..."
sleep 5
echo "Job completed!"
EOF

# Make it executable
chmod +x /tmp/my-job.sh

# Run it
cargo run --bin hodei-jobs-cli -- job run --name "My Custom Job" --script /tmp/my-job.sh
```

## Advanced Usage

### With Resources
```bash
# More CPU and memory
cargo run --bin hodei-jobs-cli -- job run --name "Heavy Job" \
    --command "sleep 300" \
    --cpu 4.0 \
    --memory 4294967296 \
    --timeout 600
```

### JSON Output
```bash
# Get output in JSON format
cargo run --bin hodei-jobs-cli -- job list --format json
cargo run --bin hodei-jobs-cli -- job get --job-id <JOB_ID> --format json
```

### Different Server
```bash
# Connect to different server
cargo run --bin hodei-jobs-cli -- job list --server http://remote-server:50051
```

## Tips

1. **Use `job run` for real-time log streaming** - Like `kubectl logs -f`
2. **Use `job queue` to just queue without streaming** - For background jobs
3. **Scripts are recommended** - Easier to write complex logic
4. **Set appropriate timeouts** - Default is 600 seconds
5. **Monitor resource usage** - Jobs have CPU and memory limits

## Troubleshooting

### Server not running
```bash
just dev  # Start the server
```

### Port already in use
```bash
just stop-server  # Kill any existing server
just dev  # Restart
```

### Job not starting
```bash
# Check server logs
just logs-server

# Check if workers are registered
cargo run --bin hodei-jobs-cli -- scheduler workers
```

### No logs appearing
```bash
# Make sure you're using `job run` (not `job queue`)
# `job run` includes log streaming
```

## Examples Location

All example scripts are in: `scripts/examples/`

- `01-hello-world.sh` - Simple demo
- `02-data-processing.sh` - ETL pipeline
- `03-ml-training.sh` - ML training
- `04-cicd-build.sh` - CI/CD pipeline
- `05-cpu-stress.sh` - Stress test
- `06-error-handling.sh` - Error handling

Feel free to copy and modify these for your own jobs!
