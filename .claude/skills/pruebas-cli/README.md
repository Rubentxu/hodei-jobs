# Pruebas CLI Skill - Hodei E2E Testing

This document explains how to use the `pruebas-cli` skill within Claude Code.

## What is a Claude Code Skill?

A **Claude Code Skill** is a specialized capability that enables Claude to perform specific tasks using standardized commands. Skills follow the Claude Code conventions and are defined by a `SKILL.md` file with YAML frontmatter.

## Installation

The skill is already installed in your Claude Code environment. It's located at:

```
.claude/skills/pruebas-cli/
```

This directory contains:
- `SKILL.md` - The skill definition file
- `README.md` - This documentation file

**Associated Script:**
- `scripts/testing/e2e_test_procedure.sh` - Automated E2E testing script

## Automated E2E Testing (Recommended)

For complete automation of the E2E testing procedure, use the standalone script:

```bash
# Run all E2E test phases
./scripts/testing/e2e_test_procedure.sh all

# Run specific phases
./scripts/testing/e2e_test_procedure.sh infra     # Phase 1: Infrastructure
./scripts/testing/e2e_test_procedure.sh build     # Phase 2: Compile
./scripts/testing/e2e_test_procedure.sh start     # Phase 3: Start Services
./scripts/testing/e2e_test_procedure.sh providers # Phase 4: Configure Providers
./scripts/testing/e2e_test_procedure.sh jobs      # Phase 5: Execute Test Jobs
./scripts/testing/e2e_test_procedure.sh validate  # Phase 6: Validation
./scripts/testing/e2e_test_procedure.sh k8s       # Phase 7: Kubernetes Testing
./scripts/testing/e2e_test_procedure.sh batch     # Phase 8: Batch Testing

# Generate report only
./scripts/testing/e2e_test_procedure.sh report
```

**Evidence Output**: `build/test-evidence/`
**Logs Output**: `logs/`

## Usage in Claude Code

To invoke the skill, use the `skill:` prefix followed by the skill name and command:

```bash
skill: pruebas-cli <command>
```

### Syntax

```
skill: pruebas-cli <command> [arguments] [options]
```

## Quick Start

### 1. Check Infrastructure

Always start by validating infrastructure is ready:

```
skill: pruebas-cli infra
```

### 2. Run a Test Job

Execute a simple test job:

```
skill: pruebas-cli run docker
```

### 3. Get the Job Report

After job completion, get the verification report:

```
skill: pruebas-cli report <job-id>
```

## Command Reference

### Infrastructure Validation

| Command | Description |
|---------|-------------|
| `skill: pruebas-cli infra` | Full infrastructure check (DB, server, Docker, K8s) |
| `skill: pruebas-cli health` | Quick health summary |

### Job Execution

| Command | Description |
|---------|-------------|
| `skill: pruebas-cli run docker` | Execute default job on Docker provider |
| `skill: pruebas-cli run kubernetes` | Execute default job on Kubernetes provider |
| `skill: pruebas-cli smoke` | Quick smoke test (single job) |
| `skill: pruebas-cli create echo` | Create simple echo job |
| `skill: pruebas-cli create fail` | Create intentional failure job |
| `skill: pruebas-cli create sleep 60` | Create long-running job (60s) |

### Reporting & Analysis

| Command | Description |
|---------|-------------|
| `skill: pruebas-cli report <job-id>` | Full verification report with findings |
| `skill: pruebas-cli errors <job-id>` | Business logic errors only |
| `skill: pruebas-cli logs <job-id>` | Job stream logs extraction |
| `skill: pruebas-cli events <job-id>` | Domain event sequence |
| `skill: pruebas-cli audit <job-id>` | Audit trail verification |
| `skill: pruebas-cli lifecycle <job-id>` | State machine coherence |
| `skill: pruebas-cli verify <job-id>` | Complete 6-point verification |
| `skill: pruebas-cli trace <job-id>` | Full traceability trace |

### Batch Operations

| Command | Description |
|---------|-------------|
| `skill: pruebas-cli batch 5` | Create and verify 5 test jobs |
| `skill: pruebas-cli findings` | All findings from recent tests |
| `skill: pruebas-cli failures` | All failed jobs with error analysis |
| `skill: pruebas-cli compare` | Docker vs Kubernetes comparison |
| `skill: pruebas-cli full` | Full E2E test suite |

## Examples

### Example 1: Quick Docker Test

```
skill: pruebas-cli infra
skill: pruebas-cli run docker
# Copy the job ID from output
skill: pruebas-cli report <job-id>
```

### Example 2: Kubernetes Failure Test

```
skill: pruebas-cli run kubernetes
# Copy the job ID
skill: pruebas-cli errors <job-id>
skill: pruebas-cli logs <job-id>
```

### Example 3: Complete Workflow

```
skill: pruebas-cli infra
skill: pruebas-cli run docker
skill: pruebas-cli run kubernetes
skill: pruebas-cli compare
```

### Example 4: Debug a Failed Job

```
# Get all findings from recent tests
skill: pruebas-cli findings

# Pick a failed job and analyze
skill: precios-cli lifecycle <job-id>
skill: pruebas-cli events <job-id>
skill: pruebas-cli audit <job-id>
```

## Command Arguments

Many commands accept optional arguments:

| Argument | Description | Example |
|----------|-------------|---------|
| `<job-id>` | UUID of the job to analyze | `123e4567-e89b-12d3-a456-426614174000` |
| `<seconds>` | Duration for sleep jobs | `60` |
| `<count>` | Number of jobs for batch | `5` |

## Job ID Format

Job IDs are UUIDs in the format:
```
123e4567-e89b-12d3-a456-426614174000
```

You can find job IDs in:
- CLI output after job creation
- `build/test-evidence/` directory
- Database queries

## Output Locations

Results and logs are stored in:

| Location | Contents |
|----------|----------|
| `build/test-evidence/` | Test results and verification evidence |
| `logs/` | CLI execution logs |
| `build/test-results/` | Structured test result files |

## Verification Procedures

The skill performs comprehensive verification following these steps:

### 1. Build CLI
```bash
cargo build -p hodei-jobs-cli --quiet
```

### 2. Infrastructure Check
```bash
docker ps --filter "name=hodei"
```

### 3. Execute Job
```bash
./target/debug/hodei-jobs-cli job run -n "test" --provider kubernetes -c "echo test"
```

### 4. Verify Events (Domain Events Table)
```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT event_type, aggregate_id, occurred_at::text FROM domain_events \
   WHERE aggregate_id = '<job-id>' ORDER BY occurred_at;"
```

**Expected Event Sequence**:
```
1. JobCreated      → Job created
2. JobAssigned     → Worker assigned
3. JobAccepted     → Job accepted
4. JobStatusChanged → RUNNING
5. JobStatusChanged → SUCCEEDED
```

### 5. Verify Job State (Jobs Table)
```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT id, state, selected_provider_id, created_at, started_at, completed_at \
   FROM jobs WHERE id = '<job-id>';"
```

### 6. Verify Log Persistence (Job_Log_Files Table)
```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT id, storage_uri, size_bytes, entry_count FROM job_log_files \
   WHERE job_id = '<job-id>';"
```

### 7. 6-Point Verification Checklist

| # | Check | Pass Criteria |
|---|-------|---------------|
| 1 | Status | `SUCCEEDED` or `FAILED` |
| 2 | Events | All events present, correct sequence |
| 3 | Logs | Entry count > 0 |
| 4 | Provider | `selected_provider_id` not null |
| 5 | Cleanup | Worker in valid state |
| 6 | Timing | `started_at` and `completed_at` set |

## Database Schema

| Table | Key Columns |
|-------|-------------|
| `jobs` | `id`, `state`, `selected_provider_id`, `created_at`, `started_at`, `completed_at` |
| `domain_events` | `event_type`, `aggregate_id`, `occurred_at`, `payload` |
| `job_log_files` | `job_id`, `storage_uri`, `size_bytes`, `entry_count` |

## Status Values

| Value | Description |
|-------|-------------|
| `CREATED` | Job created |
| `PENDING` | Waiting for worker |
| `ASSIGNED` | Worker assigned |
| `RUNNING` | Execution in progress |
| `SUCCEEDED` | Completed successfully |
| `FAILED` | Execution failed |

## Complete Execution Report

The skill generates comprehensive reports with all relevant execution data:

### Report Sections

| Section | Contents |
|---------|----------|
| **Job Information** | Job ID, name, status, provider, command, timestamps, duration |
| **Worker Information** | Worker ID, container name, provider resource ID, state |
| **Execution Context** | JSON with job_id, provider_id, execution_id, status |
| **Job Specification** | JSON with command, resources, constraints, preferences |
| **Event Flow** | Table with all events, timestamps, and status |
| **Log Stream** | Log file ID, storage URI, size, entries, and content |
| **Verification Summary** | 6-point checklist with pass/fail status |
| **Findings** | Business logic errors (if any) |

### Example Report Data

```markdown
# Job Information
| Field | Value |
|-------|-------|
| Job ID | `728af646-2834-46e1-9dcc-012636a7258a` |
| Status | `SUCCEEDED` |
| Provider | Docker |
| Duration | `5.146s` |

# Worker Information
| Field | Value |
|-------|-------|
| Worker ID | `b4055298-f750-4674-90c4-8facad362e46` |
| Container Name | `hodei-worker-b4055298-...` |
| Worker State | `TERMINATED` |

# Event Flow
| # | Event | Timestamp |
|---|-------|-----------|
| 1 | JobCreated | 16:08:47.158679+00 |
| 2 | JobAssigned | 16:08:47.238696+00 |
| 3 | JobAccepted | 16:08:47.276519+00 |
| 4 | JobStatusChanged | 16:08:47.290305+00 (RUNNING) |
| 5 | JobStatusChanged | 16:08:52.284701+00 (SUCCEEDED) |

# Log Stream
| Field | Value |
|-------|-------|
| Log File ID | `55c57acf-...` |
| Storage URI | `file:///tmp/hodei-logs/...` |
| Entry Count | `2` |

# Verification Summary
| Check | Status |
|-------|--------|
| 1. Job Status | ✅ PASS |
| 2. Event Flow | ✅ PASS |
| 3. Log Persistence | ✅ PASS |
| 4. Provider | ✅ PASS |
| 5. Worker Cleanup | ✅ PASS |
| 6. Timing | ✅ PASS |
```

1. **Always start with infrastructure check** - Validate prerequisites before running tests
2. **Use batch mode for comprehensive testing** - Run multiple jobs to catch intermittent issues
3. **Check errors first** - Use `errors <job-id>` for quick issue identification
4. **Use findings for overview** - `findings` shows all issues across recent tests
5. **Compare providers** - Use `compare` to validate both Docker and Kubernetes

## Troubleshooting

### Skill Not Recognized

If the skill is not recognized:
1. Verify the skill directory exists at `.claude/skills/pruebas-cli/`
2. Restart Claude Code to reload skills
3. Check the skill is enabled in Claude Code settings

### No Output

If a command returns no output:
1. Check infrastructure is running: `skill: pruebas-cli infra`
2. Verify the job ID is correct
3. Check logs in `logs/` directory

### Job Not Found

If a job ID is not found:
1. Verify the job exists in the database
2. Check the job ID format (UUID)
3. Run `skill: pruebas-cli findings` to see recent jobs

## See Also

- `SKILL.md` - Complete skill documentation with all commands
- `docs/PRD-pipeline-dsl.md` - Product requirements document
