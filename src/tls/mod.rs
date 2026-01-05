//! TLS configuration for OxideQ.
//!
//! Provides optional TLS support using rustls (pure Rust, no OpenSSL dependency).
//!
//! # Configuration
//!
//! TLS is enabled by setting both environment variables:
//! - `OXIDEQ_TLS_CERT` - Path to PEM-encoded certificate file
//! - `OXIDEQ_TLS_KEY` - Path to PEM-encoded private key file
//!
//! If neither is set, TLS is disabled and the server runs in HTTP mode.
//!
//! # Example
//!
//! ```ignore
//! use oxideq::tls::TlsConfig;
//!
//! // Load from environment
//! let tls = TlsConfig::from_env()?;
//!
//! // Or load directly from paths
//! let tls = TlsConfig::load("cert.pem", "key.pem")?;
//! ```

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};

/// TLS configuration holder.
#[derive(Clone)]
pub struct TlsConfig {
    /// The rustls server configuration.
    pub server_config: Arc<ServerConfig>,
}

/// Error type for TLS configuration operations.
#[derive(Debug)]
pub enum TlsError {
    /// Certificate file not found at the specified path.
    CertNotFound(String),
    /// Private key file not found at the specified path.
    KeyNotFound(String),
    /// Certificate file is invalid or cannot be parsed.
    InvalidCert(String),
    /// Private key is invalid or cannot be parsed.
    InvalidKey(String),
    /// Error building the TLS configuration.
    ConfigError(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsError::CertNotFound(p) => write!(f, "Certificate file not found: {}", p),
            TlsError::KeyNotFound(p) => write!(f, "Private key file not found: {}", p),
            TlsError::InvalidCert(e) => write!(f, "Invalid certificate: {}", e),
            TlsError::InvalidKey(e) => write!(f, "Invalid private key: {}", e),
            TlsError::ConfigError(e) => write!(f, "TLS configuration error: {}", e),
        }
    }
}

impl std::error::Error for TlsError {}

impl TlsConfig {
    /// Load TLS configuration from certificate and key files.
    ///
    /// # Arguments
    ///
    /// * `cert_path` - Path to PEM-encoded certificate file (may contain certificate chain)
    /// * `key_path` - Path to PEM-encoded private key file (RSA, ECDSA, or Ed25519)
    ///
    /// # Errors
    ///
    /// Returns `TlsError` if files cannot be found, read, or parsed.
    pub fn load(cert_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Result<Self, TlsError> {
        let cert_path = cert_path.as_ref();
        let key_path = key_path.as_ref();

        // Load certificates
        let cert_file = File::open(cert_path)
            .map_err(|_| TlsError::CertNotFound(cert_path.display().to_string()))?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Vec<CertificateDer<'static>> = certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| TlsError::InvalidCert(e.to_string()))?;

        if certs.is_empty() {
            return Err(TlsError::InvalidCert(
                "No certificates found in file".into(),
            ));
        }

        // Load private key
        let key_file = File::open(key_path)
            .map_err(|_| TlsError::KeyNotFound(key_path.display().to_string()))?;
        let mut key_reader = BufReader::new(key_file);
        let key: PrivateKeyDer<'static> = private_key(&mut key_reader)
            .map_err(|e| TlsError::InvalidKey(e.to_string()))?
            .ok_or_else(|| TlsError::InvalidKey("No private key found in file".into()))?;

        // Build server config with safe defaults
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TlsError::ConfigError(e.to_string()))?;

        Ok(Self {
            server_config: Arc::new(config),
        })
    }

    /// Try to load TLS configuration from environment variables.
    ///
    /// Reads:
    /// - `OXIDEQ_TLS_CERT` - Path to certificate file
    /// - `OXIDEQ_TLS_KEY` - Path to private key file
    ///
    /// # Returns
    ///
    /// - `Ok(Some(TlsConfig))` - Both variables set and files loaded successfully
    /// - `Ok(None)` - Neither variable is set (TLS disabled)
    /// - `Err(TlsError)` - One variable set but not the other, or file loading failed
    pub fn from_env() -> Result<Option<Self>, TlsError> {
        let cert_path = std::env::var("OXIDEQ_TLS_CERT").ok();
        let key_path = std::env::var("OXIDEQ_TLS_KEY").ok();

        match (cert_path, key_path) {
            (Some(cert), Some(key)) => Ok(Some(Self::load(&cert, &key)?)),
            (None, None) => Ok(None),
            (Some(_), None) => Err(TlsError::KeyNotFound(
                "OXIDEQ_TLS_KEY must be set when OXIDEQ_TLS_CERT is set".into(),
            )),
            (None, Some(_)) => Err(TlsError::CertNotFound(
                "OXIDEQ_TLS_CERT must be set when OXIDEQ_TLS_KEY is set".into(),
            )),
        }
    }
}

/// Check if Raft TLS is enabled via environment variable.
///
/// Reads `OXIDEQ_RAFT_TLS` and returns true if set to "true" (case-insensitive).
pub fn is_raft_tls_enabled() -> bool {
    std::env::var("OXIDEQ_RAFT_TLS")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_tls_from_env_disabled() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe {
            std::env::remove_var("OXIDEQ_TLS_CERT");
            std::env::remove_var("OXIDEQ_TLS_KEY");
        }
        let result = TlsConfig::from_env();
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    #[serial]
    fn test_tls_from_env_partial_cert_only() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe {
            std::env::set_var("OXIDEQ_TLS_CERT", "cert.pem");
            std::env::remove_var("OXIDEQ_TLS_KEY");
        }
        let result = TlsConfig::from_env();
        assert!(matches!(result, Err(TlsError::KeyNotFound(_))));
        unsafe { std::env::remove_var("OXIDEQ_TLS_CERT") };
    }

    #[test]
    #[serial]
    fn test_tls_from_env_partial_key_only() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe {
            std::env::remove_var("OXIDEQ_TLS_CERT");
            std::env::set_var("OXIDEQ_TLS_KEY", "key.pem");
        }
        let result = TlsConfig::from_env();
        assert!(matches!(result, Err(TlsError::CertNotFound(_))));
        unsafe { std::env::remove_var("OXIDEQ_TLS_KEY") };
    }

    #[test]
    #[serial]
    fn test_raft_tls_disabled_by_default() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::remove_var("OXIDEQ_RAFT_TLS") };
        assert!(!is_raft_tls_enabled());
    }

    #[test]
    #[serial]
    fn test_raft_tls_enabled() {
        // SAFETY: Tests using env vars are serialized via #[serial]
        unsafe { std::env::set_var("OXIDEQ_RAFT_TLS", "true") };
        assert!(is_raft_tls_enabled());
        unsafe { std::env::remove_var("OXIDEQ_RAFT_TLS") };
    }
}
