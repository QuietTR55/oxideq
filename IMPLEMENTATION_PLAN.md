# OxideQ Implementation Plan

This document outlines the roadmap and technical design for adding production-grade features to OxideQ.

---

## Priority 0: Security Fixes (Critical)

These issues must be addressed before production deployment.

### 0.1 Request Size Limits
**Risk:** Memory exhaustion / DoS attacks
**Location:** `src/lib.rs` - `enqueue`, `queue_batch` handlers

- [x] Add `axum::extract::DefaultBodyLimit` middleware
- [x] Set limits: 1MB for single messages, 10MB for batch operations
- [x] Return `413 Payload Too Large` when exceeded

### 0.2 Queue Name Validation
**Risk:** Path traversal, injection attacks, DoS via long names
**Location:** `src/lib.rs:50-74`, `src/storage/memory.rs:158-175`

- [x] Validate queue names: alphanumeric + underscores/hyphens only
- [x] Enforce max length (128 characters)
- [x] Reject empty strings
- [x] Return `400 Bad Request` with descriptive error

### 0.3 Raft Endpoint Security
**Risk:** Cluster takeover by network attacker
**Location:** `src/lib.rs:632-638`, `src/replication/network.rs`

- [ ] Add separate Raft authentication token (`OXIDEQ_RAFT_SECRET`)
- [ ] Validate token on all `/raft/*` endpoints
- [ ] Add TLS support for inter-node communication (future: mTLS)
- [ ] Consider moving Raft to separate port

### 0.4 Constant-Time Password Comparison
**Risk:** Timing side-channel attacks
**Location:** `src/lib.rs:44`

- [x] Replace `==` with constant-time comparison
- [x] Use `subtle` crate or similar

---

## Priority 1: Bug Fixes (High)

### 1.1 Race Conditions in Storage (TOCTOU)
**Issue:** `contains_key` followed by `remove` is not atomic
**Location:** `src/storage/memory.rs:252-270` (ack), `273-319` (nack), `321-345` (extend_visibility)

- [x] Refactor to use atomic operations (e.g., `DashMap::remove` directly, check result)
- [x] Ensure WAL is only written after confirming state exists
- [ ] Add tests for concurrent access patterns

### 1.2 WAL Replay Loses Timestamps
**Issue:** `Dequeued` replay uses `Utc::now()` instead of logged timestamp
**Location:** `src/storage/memory.rs:99-105`, `132-138`

- [ ] Store `consumed_at` timestamp in WAL entry for `Dequeued` operations
- [ ] Use logged timestamp during replay
- [ ] Add migration logic for existing WAL files (or document breaking change)

### 1.3 DLQ Creation Not Logged to WAL
**Issue:** Dead Letter Queue created implicitly but not persisted
**Location:** `src/storage/memory.rs:296-301`

- [ ] Option A: Log explicit `CreateQueue` for DLQ before inserting
- [ ] Option B: Document as intentional (DLQ recreated on nack replay)
- [ ] Add test verifying DLQ survives restart

### 1.4 Inconsistent `ack_batch` Behavior
**Issue:** `ack_batch` silently ignores missing tasks, `ack` returns error
**Location:** `src/storage/memory.rs:489-504`

- [x] Option A: Make `ack_batch` return error if any task missing
- [ ] ~~Option B: Return partial success with list of failed IDs~~
- [x] Document the chosen behavior (returns error listing all missing task IDs)

### 1.5 Cleanup Task Not Cancellable
**Issue:** Background task spawned without storing `JoinHandle`
**Location:** `src/lib.rs:586-612`

- [ ] Store `JoinHandle` from `tokio::spawn`
- [ ] Implement cancellation via `CancellationToken`
- [ ] Integrate with graceful shutdown (see Section 5)

### 1.6 Unused `_raft_addr` Variable
**Issue:** Environment variable parsed but never used
**Location:** `src/main.rs:48-49`

- [ ] Either use it to configure the Raft listener address
- [ ] Or remove if nodes get addresses from cluster config

---

## Priority 2: Performance Improvements

### 2.1 O(N*M) Stats Calculation
**Issue:** For each queue, iterates all in-flight tasks
**Location:** `src/storage/memory.rs:374-378`

- [ ] Add per-queue in-flight counter (`AtomicUsize` or `DashMap<String, AtomicUsize>`)
- [ ] Increment on dequeue, decrement on ack/nack
- [ ] Update `get_stats` to use counters

### 2.2 HTTP Client Per Raft Connection
**Issue:** Creates new `reqwest::Client` for each connection
**Location:** `src/replication/network.rs:29-31`

- [x] Create shared `Client` in `Network` struct
- [x] Pass `Arc<Client>` to `NetworkConnection`
- [x] Configure connection pool size

### 2.3 Synchronous WAL Flush Per Operation
**Issue:** Every operation triggers immediate `flush()`
**Location:** `src/storage/wal/manager.rs:31`

- [ ] Implement write buffering with periodic flush
- [ ] Add `OXIDEQ_WAL_SYNC_INTERVAL_MS` config (default: 10ms)
- [ ] Option for `fsync` vs `flush` (durability vs performance)

### 2.4 Unbounded WAL Growth
**Issue:** WAL file grows forever, no compaction
**Location:** `src/storage/wal/manager.rs`

- [ ] Implement periodic snapshotting (checkpoint)
- [ ] Truncate WAL after successful snapshot
- [ ] Add `OXIDEQ_WAL_CHECKPOINT_INTERVAL` config

### 2.5 Individual Raft Submissions for Cleanup
**Issue:** Each expired task submitted as separate Raft command
**Location:** `src/lib.rs:595-602`

- [ ] Add `ClientRequest::NackBatch { task_ids: Vec<String> }` variant
- [ ] Submit all expired tasks in single Raft entry
- [ ] Reduces Raft log entries and network round-trips

---

## 1. Replication (High Availability)

**Goal:** Ensure zero downtime if the primary server fails.
**Strategy:** Raft Consensus (via `openraft`).

> [!NOTE]
> A detailed technical design and implementation plan has been created.
> See [REPLICATION_PLAN.md](REPLICATION_PLAN.md) for the full architecture, data structures, and step-by-step guide.

*   **Chosen Approach: Raft Consensus (Robust)**
    *   Use `openraft` crate.
    *   Ensure strict consistency across a cluster of 3+ nodes.
    *   Writes are only acknowledged when committed by a quorum.
*   **High-Level Steps:**
    1.  [x] Define Raft types (`ClientRequest`, `ClientResponse`).
    2.  [x] Implement `RaftStorage` (LogStore + StateMachine).
    3.  [x] Add network endpoints for Raft communication.
    4.  [x] Route API requests through the Raft consensus layer.

### 1.1 Replication Completion (Critical Fixes)

The initial implementation is skeletal. The following steps are required to make it a functional cluster.

#### 1. Cluster Management APIs
We need endpoints to bootstrap the cluster and manage membership.
*   [x] **Create `/raft/init` endpoint:** Calls `raft.initialize(nodes)`. Used to start the very first leader.
*   [x] **Create `/raft/add-learner` endpoint:** Calls `raft.add_learner(node_id, node, blocking)`. Used to join new nodes.
*   [x] **Create `/raft/change-membership` endpoint:** Calls `raft.change_membership(members)`. Used to promote learners to voters.
*   [x] **Create `/raft/metrics` endpoint:** Expose `raft.metrics()` for monitoring/debugging.

#### 2. Snapshotting Implementation
Without snapshots, the log grows forever, and new nodes cannot join if the log is compacted.
*   [x] **Implement `CombinedStorage::build_snapshot`:**
    *   Serialize `state_machine.queues` and `state_machine.in_flight`.
    *   Return a compressed snapshot.
*   [x] **Implement `CombinedStorage::install_snapshot`:**
    *   Receive serialized data.
    *   Atomically replace `InMemoryStorage` contents with the snapshot data.
*   [ ] **Configure Snapshot Policy:** Trigger snapshot after X logs (e.g., every 5000 logs).

#### 3. Deterministic Background Cleanup
The current `cleanup_expired_tasks` runs independently on all nodes, causing state divergence.
*   [x] **Refactor `cleanup_expired_tasks`:**
    *   Run ONLY on the Leader (check `raft.metrics().state == Leader`).
    *   Instead of modifying `InMemoryStorage` directly, iterate and find expired IDs.
    *   For each expired ID, submit a `ClientRequest::Nack { task_id }` to the Raft log.
    *   This ensures the "move back to queue" operation is replicated and deterministic on all nodes.

---

## 2. Observability (Structured Logging & Metrics)

**Goal:** Gain deep visibility into system behavior and performance.

*   **Logging:**
    *   [ ] Use `tracing` ecosystem.
    *   [ ] Replace `println!` with `tracing::info!`, `error!`, `instrument`.
    *   [ ] Add `tracing-subscriber` with `EnvFilter` to control log levels via environment variables (`RUST_LOG=info,oxideq=debug`).
    *   [ ] (Optional) Export to OpenTelemetry/Jaeger via `tracing-opentelemetry`.
*   **Metrics:**
    *   [ ] Use `metrics` or `prometheus` crates.
    *   [ ] Track key counters/gauges:
        *   `queue_depth`: Total messages in queue.
        *   `messages_enqueued_total`: Counter.
        *   `messages_dequeued_total`: Counter.
        *   `request_latency_seconds`: Histogram.

---

## 3. Rate Limiting

**Goal:** Prevent abuse and service degradation from "noisy neighbors".

*   **Tools:** `tower-governor` or `governor`.
*   **Strategy:**
    *   [ ] Implement as an Axum middleware layer.
    *   [ ] **Per-IP Limiting:** Allow X reqs/sec per IP.
    *   [ ] **Per-Queue Limiting:** (Advanced) Limit rates based on the `queue_name` path parameter.
    *   [ ] Return `429 Too Many Requests` with `Retry-After` header.

---

## 4. Connection Pooling & HTTP Optimization

**Goal:** Reduce overhead of TCP handshakes and maximize throughput.

*   **Server Side (Axum/Hyper):**
    *   [x] HTTP/2 is enabled via Axum features.
    *   [ ] Tune `keep-alive` settings.
*   **Client/Consumer Side:**
    *   [ ] Use a shared `reqwest::Client` instance for Raft communication (see Priority 2.2).
*   **Database (Future):**
    *   If persistent storage (SQLite/Postgres) is added, use `deadpool` or `sqlx` connection pools.

---

## 5. Graceful Shutdown

**Goal:** Finish in-flight tasks and close file handles safely on exit.

*   **Mechanism:**
    *   [ ] Use `tokio::signal::ctrl_c` and unix signal handling (`SIGTERM`).
    *   [ ] Pass a `CancellationToken` (from `tokio-util`) to background workers (like the WAL flusher or expiration cleaner).
*   **Steps:**
    1.  [ ] Capture shutdown signal.
    2.  [ ] Stop accepting NEW HTTP requests (Axum `with_graceful_shutdown`).
    3.  [ ] Signal background tasks to stop.
    4.  [ ] Flush remaining WAL buffers to disk.
    5.  [ ] Exit process.

---

## 6. Backpressure

**Goal:** Prevent Out-Of-Memory (OOM) crashes when producers outpace consumers.

*   **Strategy:**
    *   [ ] **Input Bounding:** Set a hard limit on `max_queue_size` (e.g., 10 million messages).
    *   [ ] **Rejection:** If queue is full, `enqueue` handler returns `503 Service Unavailable` or `429`.
    *   [ ] **Channel Buffers:** Review `mpsc` channel sizes for internal async tasks. Ensure they are bounded.

---

## 7. Monitoring / Alerting

**Goal:** Know when the system is down or unhealthy.

*   **Health Check:**
    *   [x] Endpoint: `GET /health`
    *   [x] Logic: Returns `200 OK` if internal state is accessible. Returns `500` otherwise.
*   **Setup:**
    *   Deploy **Prometheus** to scrape `/metrics`.
    *   **Grafana** dashboard to visualize queue depth and error rates.
    *   **AlertManager** rules:
        *   "Queue Depth > 1M for 5 minutes" -> Warning.
        *   "Error Rate > 1%" -> Critical.
        *   "Instance Down" -> Critical.

---

## 8. Deployment Configuration

**Goal:** Reproducible, containerized deployments.

*   **Docker:**
    *   [x] Multi-stage `Dockerfile`: Build in `rust:nightly`, copy binary to `debian:bookworm-slim`.
    *   [x] `.dockerignore` to speed up builds.
    *   [x] Health check configured in Dockerfile.
*   **Docker Compose:**
    *   [x] Define `oxideq` standalone service in `docker-compose.yml`.
    *   [x] Define 3-node cluster with `--profile cluster` support.
    *   [ ] Add `prometheus`, `grafana` services (future).
*   **Kubernetes (K8s):**
    *   [ ] `Deployment` manifest with `replicas: 2`.
    *   [ ] `Service` (ClusterIP/LoadBalancer) to expose port 8540.
    *   [ ] `ConfigMap` for environment variables (`OXIDEQ_PASSWORD`, `RUST_LOG`).
    *   [ ] `LivenessProbe` pointing to `/health`.
    *   [ ] `ReadinessProbe` pointing to `/health`.

---

## 9. Load Testing

**Goal:** Validate performance claims (e.g., handling 10k req/sec).

*   **Tools:** `k6` (JS-based), `wrk` (simple/fast), or `drill` (Rust/YAML).
*   **Scenarios to Script:**
    *   [ ] **Ingest Heavy:** 100 concurrent producers pushing messages.
    *   [ ] **Drain Heavy:** 100 concurrent consumers popping messages.
    *   [ ] **Mixed:** 50/50 read/write split.
    *   [ ] **Latency check:** 99th percentile latency under load.

---

## Quick Wins (Low Effort, High Value)

- [x] Remove unused `_raft_addr` variable or implement its usage (`src/main.rs:48`)
- [x] Add `Default` trait impl for `InMemoryStorage`
- [x] Make heartbeat duration configurable (`OXIDEQ_HEARTBEAT_DURATION` env var, default 60s)
- [x] Remove unused import `serde::Deserialize` in `src/replication/combined_storage.rs:9`
- [ ] Add `#[must_use]` to functions returning `Result`
