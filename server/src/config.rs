//! Server configuration
//!
//! Configuration structures for future config file support.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use rsc_common::config::LoggingConfig;
use crate::error::{ServerError, Result};

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Server bind settings
    pub server: BindConfig,

    /// TLS settings
    pub tls: TlsConfig,

    /// vpcd settings
    #[serde(default)]
    pub vpcd: VpcdConfig,

    /// Logging settings
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Server bind configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BindConfig {
    /// Address to bind to
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,

    /// Maximum number of simultaneous clients
    #[serde(default = "default_max_clients")]
    pub max_clients: u32,
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8443
}

fn default_max_clients() -> u32 {
    100
}

/// TLS configuration for server
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    /// Path to server certificate
    pub server_cert: PathBuf,

    /// Path to server private key
    pub server_key: PathBuf,

    /// Path to CA certificate for client verification
    pub ca_cert: PathBuf,

    /// Require client certificate (mTLS)
    #[serde(default = "default_true")]
    pub require_client_cert: bool,

    /// Minimum TLS version
    #[serde(default = "default_tls_version")]
    pub min_tls_version: String,
}

fn default_true() -> bool {
    true
}

fn default_tls_version() -> String {
    "1.3".to_string()
}

/// vpcd (virtual smartcard reader) configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VpcdConfig {
    /// Path to vpcd binary
    #[serde(default = "default_vpcd_binary")]
    pub binary: PathBuf,

    /// Directory for vpcd sockets
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,

    /// Timeout for vpcd operations
    #[serde(default = "default_timeout")]
    pub timeout: String,

    /// Port range start for vpcd instances
    #[serde(default = "default_port_range_start")]
    pub port_range_start: u16,

    /// Port range end for vpcd instances
    #[serde(default = "default_port_range_end")]
    pub port_range_end: u16,
}

fn default_vpcd_binary() -> PathBuf {
    PathBuf::from("/usr/local/bin/vpcd")
}

fn default_socket_dir() -> PathBuf {
    PathBuf::from("/var/run/rsc-server/vpcd")
}

fn default_timeout() -> String {
    "30s".to_string()
}

fn default_port_range_start() -> u16 {
    35963
}

fn default_port_range_end() -> u16 {
    36963
}

impl Default for VpcdConfig {
    fn default() -> Self {
        Self {
            binary: default_vpcd_binary(),
            socket_dir: default_socket_dir(),
            timeout: default_timeout(),
            port_range_start: default_port_range_start(),
            port_range_end: default_port_range_end(),
        }
    }
}

impl ServerConfig {
    /// Load configuration from file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ServerError::Config(format!("Failed to read config file: {}", e)))?;

        let config: Self = serde_yaml::from_str(&content)
            .map_err(|e| ServerError::Config(format!("Failed to parse config: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Check that certificate files exist
        if !self.tls.server_cert.exists() {
            return Err(ServerError::Config(format!(
                "Server certificate not found: {:?}",
                self.tls.server_cert
            )));
        }

        if !self.tls.server_key.exists() {
            return Err(ServerError::Config(format!(
                "Server key not found: {:?}",
                self.tls.server_key
            )));
        }

        if !self.tls.ca_cert.exists() {
            return Err(ServerError::Config(format!(
                "CA certificate not found: {:?}",
                self.tls.ca_cert
            )));
        }

        // Check vpcd binary exists
        if !self.vpcd.binary.exists() {
            return Err(ServerError::Config(format!(
                "vpcd binary not found: {:?}",
                self.vpcd.binary
            )));
        }

        Ok(())
    }
}
