//! Telemetry and logging configuration for OxideQ.
//!
//! Supports two output formats:
//! - Human-readable (development) - default
//! - JSON (production) - enabled via OXIDEQ_LOG_FORMAT=json

use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Log output format configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable format for development
    Human,
    /// JSON format for production log aggregation
    Json,
}

impl LogFormat {
    /// Determine log format from OXIDEQ_LOG_FORMAT environment variable.
    ///
    /// Returns `Json` if the variable is set to "json" (case-insensitive),
    /// otherwise returns `Human`.
    pub fn from_env() -> Self {
        match std::env::var("OXIDEQ_LOG_FORMAT")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "json" => LogFormat::Json,
            _ => LogFormat::Human,
        }
    }
}

/// Initialize the tracing subscriber based on environment configuration.
///
/// # Environment Variables
///
/// - `OXIDEQ_LOG_FORMAT`: Output format
///   - `json` - JSON structured logging (for production)
///   - any other value - Human-readable (for development)
///
/// - `OXIDEQ_LOG_LEVEL` or `RUST_LOG`: Log level filter
///   - Examples: `info`, `debug`, `oxideq=debug,tower_http=info`
///   - Default: `info`
///
/// # Panics
///
/// Panics if the tracing subscriber has already been initialized.
pub fn init_logging() {
    let filter = EnvFilter::try_from_env("OXIDEQ_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    match LogFormat::from_env() {
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_span_events(FmtSpan::CLOSE)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false),
                )
                .init();
        }
        LogFormat::Human => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_span_events(FmtSpan::CLOSE)
                        .with_target(true)
                        .with_thread_ids(false),
                )
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_log_format_default() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::remove_var("OXIDEQ_LOG_FORMAT") };
        assert_eq!(LogFormat::from_env(), LogFormat::Human);
    }

    #[test]
    #[serial]
    fn test_log_format_json() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::set_var("OXIDEQ_LOG_FORMAT", "json") };
        assert_eq!(LogFormat::from_env(), LogFormat::Json);
        unsafe { std::env::remove_var("OXIDEQ_LOG_FORMAT") };
    }

    #[test]
    #[serial]
    fn test_log_format_json_uppercase() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::set_var("OXIDEQ_LOG_FORMAT", "JSON") };
        assert_eq!(LogFormat::from_env(), LogFormat::Json);
        unsafe { std::env::remove_var("OXIDEQ_LOG_FORMAT") };
    }
}
