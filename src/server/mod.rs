//! Server abstraction for OxideQ.
//!
//! Provides a unified interface for running HTTP and HTTPS servers with graceful shutdown.
//!
//! # Example
//!
//! ```ignore
//! use oxideq::server::run_server;
//! use oxideq::tls::TlsConfig;
//! use oxideq::shutdown::shutdown_signal;
//!
//! let tls = TlsConfig::from_env()?;
//! run_server(app, addr, tls, shutdown_signal()).await?;
//! ```

use std::net::SocketAddr;

use axum::Router;

use crate::tls::TlsConfig;

/// Run the server with optional TLS support and graceful shutdown.
///
/// # Arguments
///
/// * `router` - The Axum router to serve
/// * `addr` - Socket address to bind to
/// * `tls` - Optional TLS configuration (None for HTTP, Some for HTTPS)
/// * `shutdown_signal` - Future that completes when shutdown is requested
///
/// # Returns
///
/// Returns `Ok(())` when the server shuts down gracefully, or an error if binding fails.
pub async fn run_server(
    router: Router,
    addr: SocketAddr,
    tls: Option<TlsConfig>,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match tls {
        Some(tls_config) => {
            tracing::info!("Starting HTTPS server on https://{}", addr);

            let rustls_config =
                axum_server::tls_rustls::RustlsConfig::from_config(tls_config.server_config);

            // Create a handle for graceful shutdown
            let handle = axum_server::Handle::new();
            let shutdown_handle = handle.clone();

            // Spawn shutdown handler
            tokio::spawn(async move {
                shutdown_signal.await;
                tracing::info!("Initiating graceful shutdown for HTTPS server");
                shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
            });

            axum_server::bind_rustls(addr, rustls_config)
                .handle(handle)
                .serve(router.into_make_service())
                .await?;
        }
        None => {
            tracing::info!("Starting HTTP server on http://{}", addr);
            tracing::warn!("TLS is disabled - connections are unencrypted");

            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown_signal)
                .await?;
        }
    }

    tracing::info!("Server shutdown complete");
    Ok(())
}
