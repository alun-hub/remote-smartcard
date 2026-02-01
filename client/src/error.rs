//! Client error types

use thiserror::Error;

/// Client-specific errors
#[derive(Debug, Error)]
pub enum ClientError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// PC/SC error
    #[error("PC/SC error: {0}")]
    Pcsc(#[from] pcsc::Error),

    /// Smartcard-specific error (higher-level than PC/SC)
    #[error("Smartcard error: {0}")]
    Smartcard(String),

    /// gRPC/connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// TLS error
    #[error("TLS error: {0}")]
    Tls(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Common library error
    #[error(transparent)]
    Common(#[from] rsc_common::Error),
}

/// Result type for client operations
pub type Result<T> = std::result::Result<T, ClientError>;
