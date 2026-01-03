# API Reference

This document describes the API endpoints available in OxideQ.

## Authentication

All API requests (except `/health`) must include the `x-oxideq-password` header with the password configured in the `OXIDEQ_PASSWORD` environment variable.

```bash
curl -H "x-oxideq-password: your_password" http://localhost:8540/stats
```

## Queue Name Validation

Queue names must follow these rules:
- **Length**: 1-128 characters
- **Allowed characters**: `a-z`, `A-Z`, `0-9`, `_` (underscore), `-` (hyphen)
- **Invalid names** will return `400 Bad Request`

Valid examples: `my-queue`, `orders_v2`, `TaskQueue123`
Invalid examples: `my queue` (space), `queue/name` (slash), `` (empty)

## Request Size Limits

| Endpoint | Max Size |
| :--- | :--- |
| Single enqueue (`POST /queues/:name/tasks`) | 1 MB |
| Batch enqueue (`POST /queues/:name/tasks/batch`) | 10 MB |

Requests exceeding these limits will receive `413 Payload Too Large`.

---

## Health & Monitoring

### Health Check

Returns the health status of the service. **No authentication required.**

- **URL**: `/health`
- **Method**: `GET`
- **Success Response**:
    - **Code**: `200 OK`
    - **Body**:
    ```json
    {
      "status": "healthy"
    }
    ```
- **Error Response**:
    - **Code**: `500 Internal Server Error`
    - **Body**:
    ```json
    {
      "status": "unhealthy",
      "error": "error description"
    }
    ```

#### Example

```bash
curl http://localhost:8540/health
```

### System Stats

Returns queue statistics including pending and in-flight tasks.

- **URL**: `/stats`
- **Method**: `GET`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
    - **Body**:
    ```json
    {
      "total_in_flight": 5,
      "queues": {
        "my-queue": {
          "pending": 10,
          "in_flight": 2
        }
      }
    }
    ```

---

## Queue Operations

### Create a Queue

Creates a new named queue.

- **URL**: `/queues`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json`
    - `x-oxideq-password: <your-password>`
- **Body**:
    ```json
    {
        "name": "my-queue"
    }
    ```
- **Success Response**:
    - **Code**: `201 Created`
- **Error Responses**:
    - **Code**: `400 Bad Request` - Queue already exists or invalid name
    - **Code**: `401 Unauthorized` - Invalid password

#### Example

```bash
curl -X POST http://localhost:8540/queues \
  -H "content-type: application/json" \
  -H "x-oxideq-password: secret_password" \
  -d '{"name": "test_queue"}'
```

---

## Task Operations

### Enqueue a Task

Adds a task to a specific queue. The body can be any content (JSON, binary, text).

- **URL**: `/queues/:name/tasks`
- **Method**: `POST`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Body**: Any content (max 1 MB)
- **Success Response**:
    - **Code**: `200 OK`
- **Error Responses**:
    - **Code**: `400 Bad Request` - Invalid queue name
    - **Code**: `404 Not Found` - Queue does not exist
    - **Code**: `413 Payload Too Large` - Body exceeds 1 MB

#### Example

```bash
# JSON payload
curl -X POST http://localhost:8540/queues/test_queue/tasks \
  -H "content-type: application/json" \
  -H "x-oxideq-password: secret_password" \
  -d '{"message": "Hello World", "priority": 1}'

# Plain text
curl -X POST http://localhost:8540/queues/test_queue/tasks \
  -H "x-oxideq-password: secret_password" \
  -d 'Just a simple message'
```

### Dequeue a Task

Retrieves the oldest task from a queue (FIFO) and moves it to "in-flight" state. You must ACK the task to complete it, or NACK to retry.

- **URL**: `/queues/:name/tasks`
- **Method**: `GET`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
    - **Headers**:
        - `x-oxideq-task-id`: Unique ID for the task (required for ACK/NACK)
    - **Body**: The original message content
- **Empty Response**:
    - **Code**: `204 No Content` - Queue is empty
- **Error Responses**:
    - **Code**: `400 Bad Request` - Invalid queue name
    - **Code**: `404 Not Found` - Queue does not exist

#### Example

```bash
curl -X GET http://localhost:8540/queues/test_queue/tasks \
  -v \
  -H "x-oxideq-password: secret_password"

# Response headers will include:
# x-oxideq-task-id: 550e8400-e29b-41d4-a716-446655440000
```

### Acknowledge a Task (ACK)

Marks an in-flight task as successfully processed and removes it permanently.

- **URL**: `/queues/:name/tasks/:task_id/ack`
- **Method**: `POST`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
- **Error Response**:
    - **Code**: `404 Not Found` - Task not found or already expired

#### Example

```bash
curl -X POST http://localhost:8540/queues/test_queue/tasks/550e8400-e29b-41d4-a716-446655440000/ack \
  -H "x-oxideq-password: secret_password"
```

### Negative Acknowledge (NACK)

Marks a task as failed and immediately re-queues it at the front of the queue. After 5 failed attempts, the task is moved to the Dead Letter Queue (`{queue_name}_dlq`).

- **URL**: `/queues/:name/tasks/:task_id/nack`
- **Method**: `POST`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
- **Error Response**:
    - **Code**: `404 Not Found` - Task not found or already expired

### Extend Visibility (Heartbeat)

Extends the visibility timeout of an in-flight task, preventing it from being automatically re-queued. Use this for long-running tasks.

- **URL**: `/queues/:name/tasks/:task_id/heartbeat`
- **Method**: `POST`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
- **Error Response**:
    - **Code**: `404 Not Found` - Task not found or already expired

The extension duration is configured via `OXIDEQ_HEARTBEAT_DURATION` (default: 60 seconds).

---

## Batch Operations

OxideQ supports high-throughput batch operations for processing multiple items in a single HTTP request.

> **MessagePack Support**: All batch endpoints support `application/msgpack` for both request and response bodies. Set `Content-Type: application/msgpack` for requests and `Accept: application/msgpack` for responses.

### Batch Enqueue

Add multiple tasks to a queue in one request.

- **URL**: `/queues/:name/tasks/batch`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json` or `application/msgpack`
    - `x-oxideq-password: <your-password>`
- **Body** (JSON array, max 10 MB):
    ```json
    [
      { "foo": "bar" },
      { "baz": 123 },
      "string_payload"
    ]
    ```
- **Success Response**:
    - **Code**: `200 OK`

#### Example

```bash
curl -X POST http://localhost:8540/queues/test_queue/tasks/batch \
  -H "content-type: application/json" \
  -H "x-oxideq-password: secret_password" \
  -d '[{"task": 1}, {"task": 2}, {"task": 3}]'
```

### Batch Dequeue

Retrieve multiple tasks from a queue at once.

- **URL**: `/queues/:name/tasks/batch?count=10`
- **Method**: `GET`
- **Query Params**:
    - `count`: Number of tasks to retrieve (default: 10)
- **Headers**:
    - `accept`: `application/json` or `application/msgpack` (optional)
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
    - **Body**:
    ```json
    [
      {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "message": { "task": 1 }
      },
      {
        "id": "550e8400-e29b-41d4-a716-446655440001",
        "message": { "task": 2 }
      }
    ]
    ```

#### Example

```bash
curl -X GET "http://localhost:8540/queues/test_queue/tasks/batch?count=5" \
  -H "x-oxideq-password: secret_password"
```

### Batch Acknowledge

Acknowledge multiple tasks at once by their IDs. **All task IDs must be valid** - if any task ID is not found, the entire operation fails.

- **URL**: `/queues/:name/tasks/ack/batch`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json`
    - `x-oxideq-password: <your-password>`
- **Body**:
    ```json
    [
      "550e8400-e29b-41d4-a716-446655440000",
      "550e8400-e29b-41d4-a716-446655440001"
    ]
    ```
- **Success Response**:
    - **Code**: `200 OK`
- **Error Response**:
    - **Code**: `404 Not Found` - One or more tasks not found (returns list of missing IDs)

---

## Cluster Management (Raft)

These endpoints are used to manage a Raft cluster. Only available when `OXIDEQ_CLUSTER_MODE=true`.

### Initialize Cluster

Bootstraps the Raft cluster. **Run this only once on the first node.**

- **URL**: `/raft/init`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json`
    - `x-oxideq-password: <your-password>`
- **Body**: Map of Node ID to Node Info
    ```json
    {
      "1": { "addr": "127.0.0.1:9001" }
    }
    ```
- **Success Response**: `200 OK`

#### Example

```bash
curl -X POST http://localhost:8540/raft/init \
  -H "content-type: application/json" \
  -H "x-oxideq-password: secret_password" \
  -d '{"1": {"addr": "127.0.0.1:9001"}}'
```

### Add Learner

Adds a new node to the cluster as a Learner (non-voting member).

- **URL**: `/raft/add-learner`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json`
    - `x-oxideq-password: <your-password>`
- **Body**: Tuple of `[NodeId, NodeInfo]`
    ```json
    [2, { "addr": "127.0.0.1:9002" }]
    ```
- **Success Response**: `200 OK` (returns updated membership)

### Change Membership

Promotes learners to voters. Specify the set of node IDs that should be voting members.

- **URL**: `/raft/change-membership`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json`
    - `x-oxideq-password: <your-password>`
- **Body**: Array of Node IDs
    ```json
    [1, 2, 3]
    ```
- **Success Response**: `200 OK` (returns updated membership)

### Raft Metrics

Returns internal Raft cluster metrics including current leader, term, and log state.

- **URL**: `/raft/metrics`
- **Method**: `GET`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**: `200 OK` (JSON body with metrics)

---

## Error Responses

All endpoints may return these common error codes:

| Code | Description |
| :--- | :--- |
| `400 Bad Request` | Invalid request (malformed JSON, invalid queue name, etc.) |
| `401 Unauthorized` | Missing or invalid `x-oxideq-password` header |
| `404 Not Found` | Queue or task not found |
| `413 Payload Too Large` | Request body exceeds size limit |
| `500 Internal Server Error` | Server error |
| `503 Service Unavailable` | Cluster mode: Not the leader (response includes leader info) |
