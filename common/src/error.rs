//! Common error types for Remote Smartcard

use thiserror::Error;

/// Common error type used across the project
#[derive(Debug, Error)]
pub enum Error {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// TLS/certificate error
    #[error("TLS error: {0}")]
    Tls(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parsing error
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Result type alias using our Error
pub type Result<T> = std::result::Result<T, Error>;
