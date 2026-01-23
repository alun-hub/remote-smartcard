//! Common utilities and types for Remote Smartcard
//!
//! This crate provides shared functionality used by both the client and server:
//! - Configuration loading and validation
//! - TLS setup utilities
//! - Logging initialization
//! - Common error types

pub mod config;
pub mod error;
pub mod logging;
pub mod tls;

pub use error::{Error, Result};
