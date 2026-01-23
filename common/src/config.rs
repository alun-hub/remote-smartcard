//! Configuration types and loading utilities

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Error, Result};

/// TLS configuration shared between client and server
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    /// Path to CA certificate for verification
    pub ca_cert: PathBuf,

    /// Minimum TLS version (default: "1.3")
    #[serde(default = "default_tls_version")]
    pub min_tls_version: String,
}

fn default_tls_version() -> String {
    "1.3".to_string()
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// Log level: error, warn, info, debug, trace
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log file path (optional)
    pub file: Option<PathBuf>,

    /// Also log to stdout
    #[serde(default = "default_true")]
    pub stdout: bool,

    /// Use JSON format for logs
    #[serde(default)]
    pub json: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
            stdout: true,
            json: false,
        }
    }
}

/// Load configuration from a YAML file
pub fn load_config<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let content = std::fs::read_to_string(path)?;
    let config: T = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Parse duration from string like "5s", "30s", "1m"
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();

    if let Some(secs) = s.strip_suffix('s') {
        let secs: u64 = secs.parse().map_err(|e| Error::Config(format!("Invalid duration: {}", e)))?;
        return Ok(Duration::from_secs(secs));
    }

    if let Some(mins) = s.strip_suffix('m') {
        let mins: u64 = mins.parse().map_err(|e| Error::Config(format!("Invalid duration: {}", e)))?;
        return Ok(Duration::from_secs(mins * 60));
    }

    Err(Error::Config(format!("Invalid duration format: {}", s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }
}
