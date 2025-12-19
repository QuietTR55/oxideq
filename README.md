# OxideQ


OxideQ is a simple, high-performance, in-memory queue service written in Rust.

> [!IMPORTANT]
> **OxideQ is currently in early development.**
> 
> This project is brand new and actively being worked on. While the text-based protocol and in-memory queueing are functional, more robust features are on the roadmap, including:
> - **Write-Ahead Log (WAL)** for data durability and recovery.
> - **Disk-backed storage** to support queues larger than memory.
> - **Master/replica clustering** for high availability.
> - **Dead letter queues** for handling failed messages.
> - **Priority queues** to ensure critical tasks are processed first.
> - **Delayed and scheduled tasks** for deferred processing.
> - **WebSocket streaming** for real-time consumer delivery.
> - **Prometheus metrics** for better observability.
> - **Admin UI** for visual management and monitoring.


## Features
- **High Performance**: In-memory storage with minimal overhead.
- **Reliability**: At-least-once delivery with ACK/NACK and auto-requeue on timeout.
- **Simple API**: RESTful JSON API.
- **Observability**: `/stats` endpoint for queue monitoring.

## Documentation

Full documentation is available in the [docs](docs/README.md) directory.

- [Getting Started](docs/README.md)
- [API Reference](docs/API.md)
- [Configuration](docs/CONFIGURATION.md)

## Quick Start

```bash
# Set the password
export OXIDEQ_PASSWORD=secret

# Run the server
cargo run
```
