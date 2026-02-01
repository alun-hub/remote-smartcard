//! Client configuration
//!
//! Configuration structures for file-based configuration.
//! Supports TOML format and Windows Certificate Store integration.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use rsc_common::config::LoggingConfig;
use crate::error::{ClientError, Result};

/// Client configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ClientConfig {
    /// Server connection settings
    #[serde(default)]
    pub server: ServerConfig,

    /// TLS settings
    #[serde(default)]
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
    /// Server URL (e.g., "https://server.example.com:8443")
    #[serde(default = "default_server_url")]
    pub url: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: default_server_url(),
        }
    }
}

fn default_server_url() -> String {
    "http://127.0.0.1:8443".to_string()
}

/// TLS configuration for client
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TlsConfig {
    /// Path to CA certificate for server verification
    pub ca_cert: Option<PathBuf>,

    /// Path to client certificate (for mTLS with file-based certs)
    pub client_cert: Option<PathBuf>,

    /// Path to client private key (for mTLS with file-based certs)
    pub client_key: Option<PathBuf>,

    /// Windows Certificate Store thumbprint (for mTLS on Windows)
    /// Use this instead of client_cert/client_key on Windows
    #[cfg(windows)]
    pub windows_cert_thumbprint: Option<String>,

    /// Server name for TLS verification (SNI override)
    pub server_name: Option<String>,

    /// Skip server certificate verification (INSECURE - for testing only)
    #[serde(default)]
    pub danger_skip_verify: bool,
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

    /// Initial reconnection delay in seconds
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay: u64,

    /// Maximum reconnection delay in seconds
    #[serde(default = "default_reconnect_max_delay")]
    pub reconnect_max_delay: u64,

    /// Disable automatic reconnection
    #[serde(default)]
    pub no_reconnect: bool,

    /// Heartbeat interval in seconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
}

fn default_client_id() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn default_reconnect_delay() -> u64 {
    1
}

fn default_reconnect_max_delay() -> u64 {
    60
}

fn default_heartbeat_interval() -> u64 {
    30
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            client_id: default_client_id(),
            reader_filter: Vec::new(),
            reconnect_delay: default_reconnect_delay(),
            reconnect_max_delay: default_reconnect_max_delay(),
            no_reconnect: false,
            heartbeat_interval: default_heartbeat_interval(),
        }
    }
}

impl ClientConfig {
    /// Load configuration from TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ClientError::Config(format!("Failed to read config file: {}", e)))?;

        let config: Self = toml::from_str(&content)
            .map_err(|e| ClientError::Config(format!("Failed to parse config: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Validate TLS config if CA cert is provided (TLS enabled)
        if let Some(ca_path) = &self.tls.ca_cert {
            if !ca_path.exists() {
                return Err(ClientError::Config(format!(
                    "CA certificate not found: {:?}",
                    ca_path
                )));
            }

            // Check file-based client certs if specified
            if let Some(cert_path) = &self.tls.client_cert {
                if !cert_path.exists() {
                    return Err(ClientError::Config(format!(
                        "Client certificate not found: {:?}",
                        cert_path
                    )));
                }
            }

            if let Some(key_path) = &self.tls.client_key {
                if !key_path.exists() {
                    return Err(ClientError::Config(format!(
                        "Client key not found: {:?}",
                        key_path
                    )));
                }
            }

            // Warn if both file-based and Windows certs are specified
            #[cfg(windows)]
            if self.tls.windows_cert_thumbprint.is_some()
                && (self.tls.client_cert.is_some() || self.tls.client_key.is_some())
            {
                tracing::warn!(
                    "Both Windows cert thumbprint and file-based certs specified. \
                     Windows cert will be used."
                );
            }
        }

        Ok(())
    }

    /// Merge CLI arguments into config (CLI takes precedence)
    pub fn merge_cli_args(&mut self, args: &super::Args) {
        // Server URL
        if args.server != default_server_url() {
            self.server.url = args.server.clone();
        }

        // TLS settings
        if let Some(ca) = &args.tls_ca {
            self.tls.ca_cert = Some(ca.clone());
        }
        if let Some(cert) = &args.tls_cert {
            self.tls.client_cert = Some(cert.clone());
        }
        if let Some(key) = &args.tls_key {
            self.tls.client_key = Some(key.clone());
        }
        if let Some(name) = &args.tls_server_name {
            self.tls.server_name = Some(name.clone());
        }

        // Windows cert thumbprint
        #[cfg(windows)]
        if let Some(thumbprint) = &args.tls_windows_cert {
            self.tls.windows_cert_thumbprint = Some(thumbprint.clone());
        }

        // Client settings
        if let Some(id) = &args.client_id {
            self.client.client_id = id.clone();
        }
        if args.reconnect_delay != default_reconnect_delay() {
            self.client.reconnect_delay = args.reconnect_delay;
        }
        if args.reconnect_max_delay != default_reconnect_max_delay() {
            self.client.reconnect_max_delay = args.reconnect_max_delay;
        }
        if args.no_reconnect {
            self.client.no_reconnect = true;
        }

        // Logging
        if let Some(level) = &args.log_level {
            self.logging.level = level.clone();
        }
    }
}

/// Generate an example config file content
pub fn example_config() -> &'static str {
    r#"# Remote Smartcard Client Configuration
# Save this file as config.toml

[server]
# Server URL (use https:// for TLS)
url = "https://server.example.com:8443"

[tls]
# CA certificate for server verification (required for TLS)
ca_cert = "C:\\certs\\ca.crt"

# Option 1: File-based client certificate (cross-platform)
# client_cert = "C:\\certs\\client.crt"
# client_key = "C:\\certs\\client.key"

# Option 2: Windows Certificate Store (Windows only, more secure)
# Use thumbprint from: certmgr.msc -> Personal -> Certificates -> [cert] -> Details -> Thumbprint
# windows_cert_thumbprint = "A1B2C3D4E5F6789012345678901234567890ABCD"

# Server name for TLS verification (optional, overrides hostname from URL)
# server_name = "server.example.com"

[client]
# Unique client identifier (default: hostname)
# client_id = "my-laptop"

# Initial reconnection delay in seconds
reconnect_delay = 1

# Maximum reconnection delay in seconds (exponential backoff)
reconnect_max_delay = 60

# Disable automatic reconnection
no_reconnect = false

# Heartbeat interval in seconds
heartbeat_interval = 30

[logging]
# Log level: error, warn, info, debug, trace
level = "info"

# Log to file (optional)
# file = "C:\\logs\\rsc-client.log"

# Log to stdout
stdout = true

# Use JSON format for logs
json = false
"#
}
