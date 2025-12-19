# API Reference

This document describes the API endpoints available in OxideQ.

## Authentication

All API requests must include the `x-oxideq-password` header with the password configured in the `OXIDEQ_PASSWORD` environment variable.

## Endpoints

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
- **Error Response**:
    - **Code**: `400 Bad Request` (if queue already exists)
    - **Code**: `401 Unauthorized` (if password is incorrect)

#### Example

```bash
curl -X POST http://localhost:8540/queues \
  -H "content-type: application/json" \
  -H "x-oxideq-password: secret_password" \
  -d '{"name": "test_queue"}'
```

### Enqueue a Task

Adds a task to a specific queue.

- **URL**: `/queues/:name/tasks`
- **Method**: `POST`
- **Headers**:
    - `content-type: application/json`
    - `x-oxideq-password: <your-password>`
- **Body**:
    ```json
    {
        "message": "some task payload"
    }
    ```
- **Success Response**:
    - **Code**: `200 OK`
- **Error Response**:
    - **Code**: `404 Not Found` (if queue does not exist)
    - **Code**: `401 Unauthorized`

#### Example

```bash
curl -X POST http://localhost:8540/queues/test_queue/tasks \
  -H "content-type: application/json" \
  -H "x-oxideq-password: secret_password" \
  -d '{"message": "Hello World"}'
```

### Dequeue a Task

Retrieves the oldest task from a specific queue (FIFO) and moves it to "in-flight" state. You must ACK the task to complete it, or NACK to retry.

- **URL**: `/queues/:name/tasks`
- **Method**: `GET`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
    - **Headers**:
        - `x-oxideq-task-id`: Unique ID for the task (required for ACK/NACK)
    - **Body**: The message content as string.
- **Empty Response**:
    - **Code**: `204 No Content` (if queue is empty)
- **Error Response**:
    - **Code**: `404 Not Found` (if queue does not exist)
    - **Code**: `401 Unauthorized`

#### Example

```bash
curl -X GET http://localhost:8540/queues/test_queue/tasks \
  -v \
  -H "x-oxideq-password: secret_password"
# Note the x-oxideq-task-id header in response
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
    - **Code**: `404 Not Found` (task not found or expired)

### Negative Acknowledge (NACK)

Marks a task as failed and immediately re-queues it at the front of the queue.

- **URL**: `/queues/:name/tasks/:task_id/nack`
- **Method**: `POST`
- **Headers**:
    - `x-oxideq-password: <your-password>`
- **Success Response**:
    - **Code**: `200 OK`
- **Error Response**:
    - **Code**: `404 Not Found` (task not found or expired)

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
