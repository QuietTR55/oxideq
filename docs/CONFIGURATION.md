# Configuration

OxideQ is configured using environment variables. You can set these in your shell or use a `.env` file in the project root.

## Environment Variables

### General

| Variable | Description | Required | Default |
| :--- | :--- | :--- | :--- |
| `OXIDEQ_PASSWORD` | The password required for API authentication. All clients must send this password in the `x-oxideq-password` header. | **Yes** | N/A |
| `OXIDEQ_PORT` | The port on which the HTTP API server listens. | No | `8540` |

### Timeouts & Cleanup

| Variable | Description | Required | Default |
| :--- | :--- | :--- | :--- |
| `OXIDEQ_CLEANUP_INTERVAL` | Interval (in seconds) at which the background task checks for expired "in-flight" tasks to re-queue them. | No | `5` |
| `OXIDEQ_TASK_TIMEOUT` | Duration (in seconds) a task can remain "in-flight" before being considered timed out and re-queued (or moved to DLQ after 5 attempts). | No | `30` |
| `OXIDEQ_HEARTBEAT_DURATION` | Duration (in seconds) to extend visibility when a heartbeat is sent. | No | `60` |

### Persistence

| Variable | Description | Required | Default |
| :--- | :--- | :--- | :--- |
| `OXIDEQ_WAL_PATH` | Path to the Write-Ahead Log directory. Enabling this ensures data persists across restarts. | No | Standalone: (Memory only)<br>Cluster: `data/node_{id}` |

### Cluster / Replication

| Variable | Description | Required | Default |
| :--- | :--- | :--- | :--- |
| `OXIDEQ_CLUSTER_MODE` | Enable Raft-based clustering (`true` or `false`). | No | `false` |
| `OXIDEQ_NODE_ID` | Unique integer ID for this node in the cluster (e.g., `1`, `2`, `3`). | **Yes** (if Cluster) | N/A |
| `OXIDEQ_RAFT_ADDR` | Address for internal Raft communication (e.g., `127.0.0.1:9001`). This is used by other nodes to connect. | **Yes** (if Cluster) | N/A |

---

## Example Configurations

### Standalone (Development)

Create a `.env` file in the project root:

```env
OXIDEQ_PASSWORD=dev_password
OXIDEQ_PORT=8540
```

### Standalone with Persistence

```env
OXIDEQ_PASSWORD=dev_password
OXIDEQ_PORT=8540
OXIDEQ_WAL_PATH=./data/wal
OXIDEQ_TASK_TIMEOUT=60
OXIDEQ_HEARTBEAT_DURATION=120
```

### Production Cluster (3-Node Setup)

**Node 1** (`node1.env`):
```env
OXIDEQ_PASSWORD=prod_secret_password
OXIDEQ_PORT=8540
OXIDEQ_CLUSTER_MODE=true
OXIDEQ_NODE_ID=1
OXIDEQ_RAFT_ADDR=10.0.0.1:9000
OXIDEQ_WAL_PATH=/var/lib/oxideq/node_1
OXIDEQ_TASK_TIMEOUT=60
OXIDEQ_CLEANUP_INTERVAL=10
```

**Node 2** (`node2.env`):
```env
OXIDEQ_PASSWORD=prod_secret_password
OXIDEQ_PORT=8540
OXIDEQ_CLUSTER_MODE=true
OXIDEQ_NODE_ID=2
OXIDEQ_RAFT_ADDR=10.0.0.2:9000
OXIDEQ_WAL_PATH=/var/lib/oxideq/node_2
OXIDEQ_TASK_TIMEOUT=60
OXIDEQ_CLEANUP_INTERVAL=10
```

**Node 3** (`node3.env`):
```env
OXIDEQ_PASSWORD=prod_secret_password
OXIDEQ_PORT=8540
OXIDEQ_CLUSTER_MODE=true
OXIDEQ_NODE_ID=3
OXIDEQ_RAFT_ADDR=10.0.0.3:9000
OXIDEQ_WAL_PATH=/var/lib/oxideq/node_3
OXIDEQ_TASK_TIMEOUT=60
OXIDEQ_CLEANUP_INTERVAL=10
```

---

## Request Limits

OxideQ enforces the following limits to prevent abuse:

| Limit | Value | Description |
| :--- | :--- | :--- |
| Single message size | 1 MB | Maximum payload size for single enqueue operations |
| Batch message size | 10 MB | Maximum payload size for batch enqueue operations |
| Queue name length | 128 characters | Maximum length for queue names |
| Queue name characters | `a-z`, `A-Z`, `0-9`, `_`, `-` | Allowed characters in queue names |

---

## Dead Letter Queue (DLQ)

Tasks that fail processing 5 times are automatically moved to a Dead Letter Queue. The DLQ is automatically created with the name `{queue_name}_dlq`.

For example, if your queue is named `orders`, failed tasks will be moved to `orders_dlq` after 5 delivery attempts.
