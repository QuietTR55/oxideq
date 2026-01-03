# OxideQ

OxideQ is a high-performance, distributed, in-memory queue service written in Rust. It prioritizes speed, simplicity, and reliability.

## Features

- **Blazing Fast**: In-memory storage with minimal overhead
- **Reliable**: At-least-once delivery with ACK/NACK mechanisms
- **Persistent**: Optional Write-Ahead Logging (WAL) for data durability
- **Distributed**: Raft consensus for high availability (Cluster Mode)
- **Batch Operations**: High-throughput batch enqueue/dequeue/ack support
- **Binary Protocol**: MessagePack serialization support
- **Dead Letter Queue**: Automatic DLQ after 5 failed delivery attempts
- **Health Checks**: Built-in `/health` endpoint for monitoring

## Table of Contents

- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Docker](#using-docker)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [API Usage](#api-usage)
- [Cluster Setup](#cluster-setup)
- [Documentation](#documentation)

## Prerequisites

**Option A: Build from source**
- **Rust**: 1.70 or later
- **Cargo**: Comes with Rust

```bash
# Check Rust installation
rustc --version
cargo --version
```

**Option B: Docker**
- **Docker**: 20.10 or later
- **Docker Compose**: v2 (included with Docker Desktop)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/QuietTR55/oxideq
cd oxideq

# Build in release mode
cargo build --release

# The binary will be at ./target/release/oxideq
```

### Run Tests

```bash
cargo test
```

### Using Docker

```bash
# Build and run standalone
docker compose up -d oxideq

# Or build the image directly
docker build -t oxideq .
docker run -d -p 8540:8540 -e OXIDEQ_PASSWORD=secret oxideq
```

### Docker Cluster (3 nodes)

```bash
# Start all 3 nodes
docker compose --profile cluster up -d

# Initialize the cluster (run once after all nodes are healthy)
curl -X POST http://localhost:8541/raft/init \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: changeme" \
  -d '{"1": {"addr": "oxideq-node1:9001"}}'

# Add learners
curl -X POST http://localhost:8541/raft/add-learner \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: changeme" \
  -d '[2, {"addr": "oxideq-node2:9001"}]'

curl -X POST http://localhost:8541/raft/add-learner \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: changeme" \
  -d '[3, {"addr": "oxideq-node3:9001"}]'

# Promote to voters
curl -X POST http://localhost:8541/raft/change-membership \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: changeme" \
  -d '[1, 2, 3]'
```

## Quick Start

### 1. Set Environment Variables

Create a `.env` file in the project root:

```env
OXIDEQ_PASSWORD=your_secret_password
OXIDEQ_PORT=8540
```

Or export them directly:

```bash
export OXIDEQ_PASSWORD=your_secret_password
export OXIDEQ_PORT=8540
```

### 2. Start the Server

```bash
# Development
cargo run

# Production (use the release binary)
./target/release/oxideq
```

You should see:

```
Hello, world!
Starting in STANDALONE mode
Server running on http://0.0.0.0:8540
```

### 3. Verify It's Running

```bash
# Health check (no auth required)
curl http://localhost:8540/health

# Expected response:
# {"status":"healthy"}
```

### 4. Create a Queue

```bash
curl -X POST http://localhost:8540/queues \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: your_secret_password" \
  -d '{"name": "my-queue"}'

# Response: 201 Created
```

### 5. Enqueue a Task

```bash
curl -X POST http://localhost:8540/queues/my-queue/tasks \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: your_secret_password" \
  -d '{"message": "Hello, OxideQ!", "priority": 1}'

# Response: 200 OK
```

### 6. Dequeue and Process

```bash
# Get the next task (note the -v flag to see headers)
curl -v -X GET http://localhost:8540/queues/my-queue/tasks \
  -H "x-oxideq-password: your_secret_password"

# Response body: {"message": "Hello, OxideQ!", "priority": 1}
# Response header: x-oxideq-task-id: <task-uuid>
```

### 7. Acknowledge the Task

```bash
# Replace <task-uuid> with the actual task ID from the header
curl -X POST http://localhost:8540/queues/my-queue/tasks/<task-uuid>/ack \
  -H "x-oxideq-password: your_secret_password"

# Response: 200 OK
```

## Configuration

### Required Environment Variables

| Variable | Description |
| :--- | :--- |
| `OXIDEQ_PASSWORD` | Authentication password for API requests |

### Optional Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `OXIDEQ_PORT` | `8540` | HTTP server port |
| `OXIDEQ_CLEANUP_INTERVAL` | `5` | Seconds between expired task checks |
| `OXIDEQ_TASK_TIMEOUT` | `30` | Seconds before in-flight task expires |
| `OXIDEQ_HEARTBEAT_DURATION` | `60` | Seconds to extend visibility on heartbeat |
| `OXIDEQ_WAL_PATH` | (none) | Path for Write-Ahead Log (enables persistence) |
| `OXIDEQ_CLUSTER_MODE` | `false` | Enable Raft clustering |
| `OXIDEQ_NODE_ID` | (required if cluster) | Unique node ID (integer) |
| `OXIDEQ_RAFT_ADDR` | (required if cluster) | Raft communication address |

### Example: Standalone with Persistence

```env
OXIDEQ_PASSWORD=my_secure_password
OXIDEQ_PORT=8540
OXIDEQ_WAL_PATH=./data/wal
OXIDEQ_TASK_TIMEOUT=60
```

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for complete configuration reference.

## API Usage

### Authentication

All endpoints (except `/health`) require the `x-oxideq-password` header:

```bash
curl -H "x-oxideq-password: your_password" http://localhost:8540/stats
```

### Queue Name Rules

- Length: 1-128 characters
- Allowed: `a-z`, `A-Z`, `0-9`, `_`, `-`

### Common Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| GET | `/health` | Health check (no auth) |
| GET | `/stats` | Queue statistics |
| POST | `/queues` | Create a queue |
| POST | `/queues/:name/tasks` | Enqueue a task |
| GET | `/queues/:name/tasks` | Dequeue a task |
| POST | `/queues/:name/tasks/:id/ack` | Acknowledge task |
| POST | `/queues/:name/tasks/:id/nack` | Negative acknowledge |
| POST | `/queues/:name/tasks/:id/heartbeat` | Extend visibility |

### Batch Operations

```bash
# Batch enqueue
curl -X POST http://localhost:8540/queues/my-queue/tasks/batch \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: your_password" \
  -d '[{"task": 1}, {"task": 2}, {"task": 3}]'

# Batch dequeue
curl -X GET "http://localhost:8540/queues/my-queue/tasks/batch?count=10" \
  -H "x-oxideq-password: your_password"

# Batch acknowledge
curl -X POST http://localhost:8540/queues/my-queue/tasks/ack/batch \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: your_password" \
  -d '["task-id-1", "task-id-2"]'
```

See [docs/API.md](docs/API.md) for complete API reference.

## Cluster Setup

OxideQ supports Raft-based clustering for high availability.

### 1. Start Three Nodes

**Node 1:**
```bash
OXIDEQ_PASSWORD=cluster_secret \
OXIDEQ_PORT=8541 \
OXIDEQ_CLUSTER_MODE=true \
OXIDEQ_NODE_ID=1 \
OXIDEQ_RAFT_ADDR=127.0.0.1:9001 \
cargo run
```

**Node 2:**
```bash
OXIDEQ_PASSWORD=cluster_secret \
OXIDEQ_PORT=8542 \
OXIDEQ_CLUSTER_MODE=true \
OXIDEQ_NODE_ID=2 \
OXIDEQ_RAFT_ADDR=127.0.0.1:9002 \
cargo run
```

**Node 3:**
```bash
OXIDEQ_PASSWORD=cluster_secret \
OXIDEQ_PORT=8543 \
OXIDEQ_CLUSTER_MODE=true \
OXIDEQ_NODE_ID=3 \
OXIDEQ_RAFT_ADDR=127.0.0.1:9003 \
cargo run
```

### 2. Initialize the Cluster (on Node 1)

```bash
curl -X POST http://localhost:8541/raft/init \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: cluster_secret" \
  -d '{"1": {"addr": "127.0.0.1:9001"}}'
```

### 3. Add Learners

```bash
# Add Node 2
curl -X POST http://localhost:8541/raft/add-learner \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: cluster_secret" \
  -d '[2, {"addr": "127.0.0.1:9002"}]'

# Add Node 3
curl -X POST http://localhost:8541/raft/add-learner \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: cluster_secret" \
  -d '[3, {"addr": "127.0.0.1:9003"}]'
```

### 4. Promote to Voters

```bash
curl -X POST http://localhost:8541/raft/change-membership \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: cluster_secret" \
  -d '[1, 2, 3]'
```

### 5. Check Cluster Status

```bash
curl http://localhost:8541/raft/metrics \
  -H "x-oxideq-password: cluster_secret"
```

## Request Limits

| Limit | Value |
| :--- | :--- |
| Single message | 1 MB |
| Batch message | 10 MB |
| Queue name | 128 characters |

## Dead Letter Queue

Tasks that fail 5 times are automatically moved to `{queue_name}_dlq`. Process DLQ tasks like any other queue.

## Documentation

- [Getting Started](docs/README.md)
- [API Reference](docs/API.md)
- [Configuration](docs/CONFIGURATION.md)

## License

MIT
