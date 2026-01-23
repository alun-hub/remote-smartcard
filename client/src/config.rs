//! Client configuration

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use rsc_common::config::LoggingConfig;
use crate::error::{ClientError, Result};

/// Client configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientConfig {
    /// Server connection settings
    pub server: ServerConfig,

    /// TLS settings
    pub tls: TlsConfig,

    /// Client behavior settings
    #[serde(default)]
    pub client: ClientSettings,

    /// Logging settings
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Server connection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Server hostname or IP address
    pub host: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    8443
}

/// TLS configuration for client
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    /// Path to client certificate
    pub client_cert: PathBuf,

    /// Path to client private key
    pub client_key: PathBuf,

    /// Path to CA certificate for server verification
    pub ca_cert: PathBuf,

    /// Verify server certificate (should always be true in production)
    #[serde(default = "default_true")]
    pub verify_server: bool,
}

fn default_true() -> bool {
    true
}

/// Client behavior settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientSettings {
    /// Unique client identifier
    #[serde(default = "default_client_id")]
    pub client_id: String,

    /// Filter which readers to forward (empty = all)
    #[serde(default)]
    pub reader_filter: Vec<String>,

    /// Reconnect interval (e.g., "5s")
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval: String,

    /// Maximum reconnect attempts (0 = infinite)
    #[serde(default)]
    pub reconnect_max_attempts: u32,

    /// Heartbeat interval (e.g., "30s")
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: String,

    /// Operation timeout (e.g., "10s")
    #[serde(default = "default_operation_timeout")]
    pub operation_timeout: String,
}

fn default_client_id() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn default_reconnect_interval() -> String {
    "5s".to_string()
}

fn default_heartbeat_interval() -> String {
    "30s".to_string()
}

fn default_operation_timeout() -> String {
    "10s".to_string()
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            client_id: default_client_id(),
            reader_filter: Vec::new(),
            reconnect_interval: default_reconnect_interval(),
            reconnect_max_attempts: 0,
            heartbeat_interval: default_heartbeat_interval(),
            operation_timeout: default_operation_timeout(),
        }
    }
}

impl ClientConfig {
    /// Load configuration from file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ClientError::Config(format!("Failed to read config file: {}", e)))?;

        let config: Self = serde_yaml::from_str(&content)
            .map_err(|e| ClientError::Config(format!("Failed to parse config: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Check that certificate files exist
        if !self.tls.client_cert.exists() {
            return Err(ClientError::Config(format!(
                "Client certificate not found: {:?}",
                self.tls.client_cert
            )));
        }

        if !self.tls.client_key.exists() {
            return Err(ClientError::Config(format!(
                "Client key not found: {:?}",
                self.tls.client_key
            )));
        }

        if !self.tls.ca_cert.exists() {
            return Err(ClientError::Config(format!(
                "CA certificate not found: {:?}",
                self.tls.ca_cert
            )));
        }

        Ok(())
    }
}
