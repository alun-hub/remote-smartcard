//! Server error types

use thiserror::Error;

/// Server-specific errors
#[derive(Debug, Error)]
pub enum ServerError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// gRPC/connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// TLS error
    #[error("TLS error: {0}")]
    Tls(String),

    /// vpcd error
    #[error("vpcd error: {0}")]
    Vpcd(String),

    /// Session error
    #[error("Session error: {0}")]
    Session(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Common library error
    #[error(transparent)]
    Common(#[from] rsc_common::Error),
}

/// Result type for server operations
pub type Result<T> = std::result::Result<T, ServerError>;
