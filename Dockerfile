# Build stage
# Using nightly because Cargo.toml specifies edition = "2024"
FROM rustlang/rust:nightly-bookworm AS builder

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy actual source code
COPY src ./src

# Touch main.rs to force rebuild of our code (not dependencies)
RUN touch src/main.rs

# Build the release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install ca-certificates and curl for HTTPS and health checks
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false oxideq

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/oxideq /app/oxideq

# Create data directory for WAL
RUN mkdir -p /app/data && chown -R oxideq:oxideq /app

USER oxideq

# Default port
EXPOSE 8540

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8540/health || exit 1

# Default command
CMD ["/app/oxideq"]
