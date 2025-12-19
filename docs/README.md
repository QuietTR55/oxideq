# OxideQ Documentation

Welcome to the OxideQ documentation.

## Overview

OxideQ is a simple, high-performance, in-memory queue service written in Rust. It provides a RESTful API for creating queues, enqueuing messages, and dequeuing messages.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)

## Installation

1.  Clone the repository:
    ```bash
    git clone https://github.com/yourusername/oxideq.git
    cd oxideq
    ```

2.  Build the project:
    ```bash
    cargo build --release
    ```

## Usage

1.  Start the server:
    ```bash
    # Set the required password environment variable
    export OXIDEQ_PASSWORD=your_secure_password
    cargo run
    ```
    Or run a built binary
    ```bash
     export OXIDEQ_PASSWORD=your_secure_password
     ./target/release/oxideq
    ```

2.  The server will start on `http://0.0.0.0:8540` by default.

## Documentation Sections

- [API Reference](API.md) - detailed information about the API endpoints.
- [Configuration](CONFIGURATION.md) - information about configuration options.
