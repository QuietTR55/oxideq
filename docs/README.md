# OxideQ Documentation

Welcome to the OxideQ documentation. This guide will help you get started with OxideQ, a high-performance, distributed, in-memory queue service written in Rust.

## Overview

OxideQ provides:

- **RESTful HTTP API** for queue operations
- **In-memory storage** with optional WAL persistence
- **At-least-once delivery** with ACK/NACK semantics
- **Visibility timeouts** with heartbeat extension
- **Dead Letter Queue** for failed messages
- **Batch operations** for high throughput
- **Raft clustering** for high availability

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.70 or later
- curl or any HTTP client for testing

## Quick Installation

```bash
# Clone the repository
git clone https://github.com/your-org/oxideq.git
cd oxideq

# Build the project
cargo build --release

# The binary is at ./target/release/oxideq
```

## Running OxideQ

### Standalone Mode (Development)

The simplest way to run OxideQ:

```bash
# Set the required password
export OXIDEQ_PASSWORD=my_password

# Start the server
cargo run
```

### Standalone Mode with Persistence

Enable Write-Ahead Log to persist data across restarts:

```bash
export OXIDEQ_PASSWORD=my_password
export OXIDEQ_WAL_PATH=./data/wal

cargo run
```

### Production (Release Binary)

```bash
export OXIDEQ_PASSWORD=my_secure_password
./target/release/oxideq
```

## Connecting to OxideQ

### Health Check

Verify the server is running (no authentication required):

```bash
curl http://localhost:8540/health
# {"status":"healthy"}
```

### Basic Workflow

```bash
# 1. Create a queue
curl -X POST http://localhost:8540/queues \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: my_password" \
  -d '{"name": "tasks"}'

# 2. Add a task
curl -X POST http://localhost:8540/queues/tasks/tasks \
  -H "Content-Type: application/json" \
  -H "x-oxideq-password: my_password" \
  -d '{"job": "process_order", "order_id": 12345}'

# 3. Get a task (note the -v to see headers)
curl -v -X GET http://localhost:8540/queues/tasks/tasks \
  -H "x-oxideq-password: my_password"
# Look for: x-oxideq-task-id header

# 4. Acknowledge completion
curl -X POST http://localhost:8540/queues/tasks/tasks/TASK_ID_HERE/ack \
  -H "x-oxideq-password: my_password"
```

## Task Lifecycle

```
┌─────────────┐     dequeue     ┌─────────────┐
│   PENDING   │ ───────────────▶│  IN-FLIGHT  │
│  (in queue) │                 │ (processing)│
└─────────────┘                 └──────┬──────┘
      ▲                                │
      │                    ┌───────────┼───────────┐
      │                    │           │           │
      │ nack/timeout       │ ack       │ 5x fail   │
      │ (retry < 5)        │           │           │
      │                    ▼           │           ▼
      └────────────────────┘           │     ┌─────────┐
                                       │     │   DLQ   │
                              ┌────────┘     └─────────┘
                              ▼
                         ┌─────────┐
                         │ DELETED │
                         └─────────┘
```

## Documentation Sections

- **[API Reference](API.md)** - Complete endpoint documentation with examples
- **[Configuration](CONFIGURATION.md)** - Environment variables and settings

## Environment Variables Summary

| Variable | Required | Default | Description |
| :--- | :--- | :--- | :--- |
| `OXIDEQ_PASSWORD` | Yes | - | API authentication password |
| `OXIDEQ_PORT` | No | 8540 | Server port |
| `OXIDEQ_WAL_PATH` | No | - | Path for persistence |
| `OXIDEQ_TASK_TIMEOUT` | No | 30 | Seconds until task expires |
| `OXIDEQ_CLEANUP_INTERVAL` | No | 5 | Cleanup check interval |
| `OXIDEQ_HEARTBEAT_DURATION` | No | 60 | Heartbeat extension |
| `OXIDEQ_CLUSTER_MODE` | No | false | Enable Raft clustering |
| `OXIDEQ_NODE_ID` | If cluster | - | Node identifier |
| `OXIDEQ_RAFT_ADDR` | If cluster | - | Raft address |

## Client Libraries

OxideQ uses a standard HTTP API. You can use any HTTP client:

### Python

```python
import requests

BASE_URL = "http://localhost:8540"
HEADERS = {"x-oxideq-password": "my_password"}

# Create queue
requests.post(f"{BASE_URL}/queues",
    json={"name": "my-queue"},
    headers=HEADERS)

# Enqueue
requests.post(f"{BASE_URL}/queues/my-queue/tasks",
    json={"data": "hello"},
    headers=HEADERS)

# Dequeue
response = requests.get(f"{BASE_URL}/queues/my-queue/tasks",
    headers=HEADERS)
if response.status_code == 200:
    task_id = response.headers["x-oxideq-task-id"]
    data = response.json()
    # Process...
    requests.post(f"{BASE_URL}/queues/my-queue/tasks/{task_id}/ack",
        headers=HEADERS)
```

### Node.js

```javascript
const axios = require('axios');

const client = axios.create({
  baseURL: 'http://localhost:8540',
  headers: { 'x-oxideq-password': 'my_password' }
});

// Create queue
await client.post('/queues', { name: 'my-queue' });

// Enqueue
await client.post('/queues/my-queue/tasks', { data: 'hello' });

// Dequeue
const response = await client.get('/queues/my-queue/tasks');
if (response.status === 200) {
  const taskId = response.headers['x-oxideq-task-id'];
  const data = response.data;
  // Process...
  await client.post(`/queues/my-queue/tasks/${taskId}/ack`);
}
```

## Next Steps

1. Read the [API Reference](API.md) for all available endpoints
2. Configure your environment with [Configuration Guide](CONFIGURATION.md)
3. Set up a production cluster (see main README)
