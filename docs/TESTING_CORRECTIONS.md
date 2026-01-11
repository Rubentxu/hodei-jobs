# Testing Corrections and Cleanup Issues

## Document Purpose
Track all corrections made during E2E testing phase and remaining issues to fix.

---

## ✅ Fixed Issues

### 1. DockerProvider Idempotency (destroy_worker)
**Date**: 2026-01-10  
**Severity**: HIGH  
**Component**: `crates/server/infrastructure/src/providers/docker.rs`

**Problem**:
- When destroying orphan workers whose Docker containers were already removed, `destroy_worker()` failed with "No such container" error
- This caused cascading failures in worker cleanup and garbage collection
- Workers remained in DB as "BUSY" even though their containers were gone
- Jobs got stuck in "ASSIGNED" state assigned to dead workers

**Root Cause**:
```rust
// Before: Not idempotent
self.client
    .remove_container(container_id, Some(remove_options))
    .await
    .map_err(|e| ProviderError::Internal(format!("Failed to remove container: {}", e)))?;
```

**Solution Applied**:
```rust
// After: Idempotent - treats "container not found" as success
if let Err(e) = self.client
    .remove_container(container_id, Some(remove_options))
    .await
{
    let err_msg = e.to_string();
    if !err_msg.contains("No such container") && !err_msg.contains("404") {
        return Err(ProviderError::Internal(format!("Failed to remove container: {}", e)));
    }
    debug!("Container {} already removed, treating as success", container_id);
}
```

**Impact**:
- Workers can now be cleaned up from DB even if containers are already gone
- Garbage collector can successfully clean orphan workers
- Reduced error noise in logs (no more repeated "No such container" warnings)

**Files Modified**:
- `crates/server/infrastructure/src/providers/docker.rs`

---

### 2. Missing Database Column (updated_at in jobs table)
**Date**: 2026-01-10  
**Severity**: CRITICAL  
**Component**: Database Schema / PostgreSQL

**Problem**:
- ExecuteJob saga step failed with: `column "updated_at" of relation "jobs" does not exist`
- This prevented jobs from transitioning from PENDING → RUNNING → SUCCEEDED
- Saga compensation was triggered, rolling back job execution

**Root Cause**:
- Code expected `updated_at` column but database schema was missing it
- Likely from migration mismatch or incomplete schema update

**Solution Applied**:
```sql
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW();
```

**Impact**:
- Jobs can now transition through all states successfully
- ExecutionSaga completes without errors
- Full E2E flow now works: QUEUED → PENDING → ASSIGNED → RUNNING → SUCCEEDED

**Files Modified**:
- Database schema (manual ALTER TABLE, should be added to migrations)

**TODO**:
- [ ] Create proper migration file for `updated_at` column
- [ ] Verify all tables have `updated_at` and `created_at` columns consistently

---

## ⚠️ Remaining Issues

### 3. Log Streaming Not Working
**Status**: INVESTIGATING  
**Severity**: HIGH  
**Component**: Log Ingestion / gRPC Streaming

**Problem**:
- Job executes successfully and completes (state = SUCCEEDED)
- Server receives logs from worker
- CLI subscribes to log stream but receives no logs
- Job completes before CLI can display any output

**Observed Behavior**:
```
✅ Job queued successfully!
🚀 Streaming logs in real-time...
   Timeout: 15s (will abort if no activity)
[no logs appear, timeout after 15s]
```

**Server Logs Show**:
- ✅ Worker sends logs via `WorkerPayload::Log` and `WorkerPayload::LogBatch`
- ✅ Server processes logs: "Received log batch for job..."
- ✅ LogIngestor processes entries
- ❓ Unknown if logs are persisted to storage
- ❓ Unknown if log stream consumers are notified

**Potential Root Causes**:
1. Logs arrive after job completes, stream already closed
2. Log storage/retrieval issue (not persisted or query fails)
3. gRPC stream subscription timing issue (CLI subscribes too late)
4. Log routing issue (logs go to wrong job_id or stream)

**Investigation Steps**:
- [ ] Check if logs are persisted to DB (query `job_log_files` table)
- [ ] Verify log stream subscription happens before job execution
- [ ] Add debug logs in log streaming path (server → CLI)
- [ ] Check if `LogIngestor` publishes events to subscribers
- [ ] Verify gRPC stream stays open until job completes

**Files to Review**:
- `crates/server/interface/src/grpc/worker.rs` (log ingestion)
- `crates/server/infrastructure/src/logging/log_service.rs` (log storage)
- `crates/server/interface/src/grpc/job.rs` (log streaming to CLI)
- `crates/cli/src/commands/job.rs` (CLI log subscription)

---

### 4. Worker Cleanup Not Fully Automatic
**Status**: DESIGN ISSUE  
**Severity**: MEDIUM  
**Component**: Worker Lifecycle / Garbage Collection

**Problem**:
- When workers crash or are manually killed, they remain in DB
- Jobs assigned to crashed workers get stuck in "ASSIGNED" state
- Manual cleanup required (stop containers, DELETE FROM workers)

**Current Behavior**:
- ✅ Heartbeat timeout detection exists (WorkerSupervisor)
- ✅ Garbage collector can detect orphan workers
- ❌ Automatic job reassignment not triggered on worker failure
- ❌ Stale workers not automatically marked as TERMINATED

**Expected Behavior** (per LIFECYCLE_UNIFIED.md):
1. Worker misses heartbeat → WorkerHeartbeatMissed event
2. After grace period → Worker marked FAILED/TERMINATED
3. Job reassignment triggered automatically
4. Worker resources cleaned up (DB + container)

**Investigation Steps**:
- [ ] Review WorkerMonitor heartbeat timeout logic
- [ ] Check if WorkerHeartbeatMissed events trigger job reassignment
- [ ] Verify compensation saga runs when worker fails
- [ ] Add automatic recovery saga for stuck jobs

**Files to Review**:
- `crates/server/application/src/workers/lifecycle.rs`
- `crates/server/application/src/workers/garbage_collector.rs`
- `crates/server/application/src/workers/actor.rs`
- `docs/analysis/LIFECYCLE_UNIFIED.md` (reference)
- `docs/analysis/LIFECYCLE_GRAPH_ANALYSIS.md` (reference)

---

## 📋 Test Execution Checklist

### Docker Provider Tests
- [ ] `just job-docker-hello` - Simple echo command
- [ ] `just job-docker-cpu` - CPU-intensive job
- [ ] `just job-docker-memory` - Memory-intensive job
- [ ] `just job-docker-data` - Data processing (ephemeral)
- [ ] `just job-docker-build` - CI/CD build simulation
- [ ] `just job-docker-ml` - ML training (small model)
- [ ] `just job-docker-all` - All Docker jobs sequentially

### Kubernetes Provider Tests
- [ ] `just job-k8s-hello` - Simple echo command
- [ ] `just job-k8s-cpu` - CPU-intensive job (scalable)
- [ ] `just job-k8s-memory` - Memory-intensive job
- [ ] `just job-k8s-data` - Data processing (scalable)
- [ ] `just job-k8s-build` - CI/CD build (full pipeline)
- [ ] `just job-k8s-ml` - ML training (large model)
- [ ] `just job-k8s-gpu` - GPU job (requires GPU cluster)
- [ ] `just job-k8s-all` - All K8s jobs sequentially

### Error Handling Tests
- [ ] Job with invalid command
- [ ] Job timeout
- [ ] Worker crash during execution
- [ ] Worker heartbeat timeout
- [ ] Resource exhaustion (OOM, disk full)

### Cleanup Tests
- [ ] Orphan worker cleanup
- [ ] Stale job reassignment
- [ ] Container cleanup after job completion
- [ ] Database cleanup (completed jobs, old logs)

---

## 🎯 Success Criteria

Each test job MUST:
1. ✅ Complete within 15 seconds (or abort for investigation)
2. ✅ Show execution traces in server logs
3. ✅ Display real-time logs in CLI
4. ✅ Reach SUCCEEDED or FAILED state (no stuck jobs)
5. ✅ Clean up worker resources automatically
6. ✅ Leave no orphan containers
7. ✅ Leave no stale DB records (except completed jobs for audit)

---

## 📊 Testing Progress

### Summary
- **Fixed**: 2 critical issues
- **Remaining**: 2 high-priority issues
- **Tests Passing**: 0/14 Docker, 0/8 K8s
- **Next Focus**: Fix log streaming, then run full test suite

### Latest Test Run
**Date**: 2026-01-10  
**Test**: `just job-docker-hello`  
**Result**: ⚠️ PARTIAL SUCCESS

**What Worked**:
- ✅ Job queued successfully
- ✅ Worker provisioned and registered
- ✅ WorkerReady event emitted with job_id
- ✅ ExecutionSaga executed (ValidateJob → ExecuteJob → CompleteJob)
- ✅ Job reached SUCCEEDED state
- ✅ Worker cleanup initiated

**What Failed**:
- ❌ No logs displayed in CLI
- ❌ Test timed out after 15s waiting for logs

**Server Logs Excerpt**:
```
✅ Job validation successful job_id=87d9a02f state=Pending
✅ Job execution started successfully job_id=87d9a02f worker_id=df3d2a49
✅ Job completed successfully job_id=87d9a02f final_state=Succeeded
✅ Saga completed successfully job_id=87d9a02f
```

---

## 🔧 Recommended Actions

### Immediate (Critical Path)
1. **Fix log streaming** - Without logs, we can't verify job execution details
2. **Test full Docker suite** - Verify all Docker provider jobs work end-to-end
3. **Document migration** - Create proper migration for `updated_at` column

### Short-term (This Sprint)
4. **Implement automatic job reassignment** - When worker fails/times out
5. **Add integration tests** - Automated tests for EDA flow
6. **Clean up warnings** - Fix unused imports and dead code

### Medium-term (Next Sprint)
7. **Improve error handling** - Better compensation sagas
8. **Add observability** - Structured logs, OpenTelemetry traces
9. **Performance testing** - Test with 10+ concurrent jobs

---

## 📝 Notes

### Architecture Compliance
The fixes follow the documented architecture:
- **EDA Pattern**: Consumers delegate to Application dispatchers ✅
- **SRP**: Each component has single responsibility ✅
- **Idempotency**: Operations are safe to retry ✅
- **Saga Pattern**: Orchestration with compensation ✅

### Reference Documents
- `docs/analysis/LIFECYCLE_UNIFIED.md` - Lifecycle state machines
- `docs/analysis/LIFECYCLE_GRAPH_ANALYSIS.md` - Event-Command-Event pattern
- `docs/REFACTORING_EXECUTION_SAGA.md` - ExecutionSaga refactoring details

---

**Last Updated**: 2026-01-10  
**Next Review**: After fixing log streaming issue