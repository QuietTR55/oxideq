//! Graceful shutdown handling for OxideQ.
//!
//! Provides signal handling and coordinated shutdown with configurable drain timeout.
//!
//! # Supported Signals
//!
//! - `SIGTERM` (Unix) - Standard termination signal from container orchestrators
//! - `SIGINT` / Ctrl+C - Interactive termination
//!
//! # Example
//!
//! ```ignore
//! use oxideq::shutdown::shutdown_signal;
//!
//! axum::serve(listener, app)
//!     .with_graceful_shutdown(shutdown_signal())
//!     .await?;
//! ```

use tokio::signal;

/// Default drain timeout in seconds.
pub const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 10;

/// Create a future that completes when a shutdown signal is received.
///
/// Listens for:
/// - `Ctrl+C` (all platforms)
/// - `SIGTERM` (Unix only)
///
/// This function is designed to be passed to `axum::serve().with_graceful_shutdown()`.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown");
        }
    }
}

/// Get the configured drain timeout from environment.
///
/// Reads `OXIDEQ_DRAIN_TIMEOUT` environment variable.
/// Falls back to `DEFAULT_DRAIN_TIMEOUT_SECS` (10 seconds) if not set or invalid.
pub fn get_drain_timeout_secs() -> u64 {
    std::env::var("OXIDEQ_DRAIN_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_DRAIN_TIMEOUT_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_default_drain_timeout() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::remove_var("OXIDEQ_DRAIN_TIMEOUT") };
        assert_eq!(get_drain_timeout_secs(), DEFAULT_DRAIN_TIMEOUT_SECS);
    }

    #[test]
    #[serial]
    fn test_custom_drain_timeout() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::set_var("OXIDEQ_DRAIN_TIMEOUT", "30") };
        assert_eq!(get_drain_timeout_secs(), 30);
        unsafe { std::env::remove_var("OXIDEQ_DRAIN_TIMEOUT") };
    }

    #[test]
    #[serial]
    fn test_invalid_drain_timeout_fallback() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::set_var("OXIDEQ_DRAIN_TIMEOUT", "invalid") };
        assert_eq!(get_drain_timeout_secs(), DEFAULT_DRAIN_TIMEOUT_SECS);
        unsafe { std::env::remove_var("OXIDEQ_DRAIN_TIMEOUT") };
    }
}
