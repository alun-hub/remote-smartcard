//! Remote Smartcard Client
//!
//! Connects to a local smartcard via PC/SC and forwards operations
//! to a remote rsc-server over gRPC with mTLS.

use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::watch;
use tonic::transport::ClientTlsConfig;
use tracing::{info, error, warn, debug};

mod config;
mod error;
mod grpc_client;
mod pcsc_reader;

use error::{ClientError, Result};
use grpc_client::GrpcClient;

/// Remote Smartcard Client
#[derive(Parser, Debug, Clone)]
#[command(name = "rsc-client")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override log level
    #[arg(short, long)]
    log_level: Option<String>,

    /// Server address (use https:// for TLS)
    #[arg(short, long, default_value = "http://127.0.0.1:8443")]
    server: String,

    /// Client ID
    #[arg(long)]
    client_id: Option<String>,

    // TLS options
    /// CA certificate for server verification (PEM format)
    #[arg(long)]
    tls_ca: Option<PathBuf>,

    /// Client certificate file (PEM format) - for mTLS
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// Client private key file (PEM format) - for mTLS
    #[arg(long)]
    tls_key: Option<PathBuf>,

    /// Server name for TLS verification (SNI)
    #[arg(long)]
    tls_server_name: Option<String>,

    // Reconnection options
    /// Initial reconnection delay in seconds
    #[arg(long, default_value = "1")]
    reconnect_delay: u64,

    /// Maximum reconnection delay in seconds
    #[arg(long, default_value = "60")]
    reconnect_max_delay: u64,

    /// Disable automatic reconnection
    #[arg(long)]
    no_reconnect: bool,
}

/// Connection state (reserved for future use)
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Reconnection configuration
struct ReconnectConfig {
    initial_delay: Duration,
    max_delay: Duration,
    current_delay: Duration,
    enabled: bool,
}

impl ReconnectConfig {
    fn new(initial_delay: Duration, max_delay: Duration, enabled: bool) -> Self {
        Self {
            initial_delay,
            max_delay,
            current_delay: initial_delay,
            enabled,
        }
    }

    /// Get current delay and increase for next attempt (exponential backoff)
    fn next_delay(&mut self) -> Duration {
        let delay = self.current_delay;
        // Double the delay for next time, up to max
        self.current_delay = std::cmp::min(self.current_delay * 2, self.max_delay);
        delay
    }

    /// Reset delay after successful connection
    fn reset(&mut self) {
        self.current_delay = self.initial_delay;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = args.log_level.clone().unwrap_or_else(|| "info".to_string());
    let log_config = rsc_common::config::LoggingConfig {
        level: log_level,
        file: None,
        stdout: true,
        json: false,
    };
    rsc_common::logging::init_logging(&log_config)?;

    info!("Starting rsc-client v{}", env!("CARGO_PKG_VERSION"));

    // Get client ID
    let client_id = args.client_id.clone().unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown-client".to_string())
    });

    // Build TLS configuration if CA is provided
    let tls_config = build_tls_config(&args)?;

    // Setup shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Handle Ctrl+C
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal");
        let _ = shutdown_tx_clone.send(true);
    });

    // Reconnection configuration
    let mut reconnect_config = ReconnectConfig::new(
        Duration::from_secs(args.reconnect_delay),
        Duration::from_secs(args.reconnect_max_delay),
        !args.no_reconnect,
    );

    // Main connection loop
    loop {
        match run_connection(&args.server, &client_id, tls_config.clone(), shutdown_rx.clone()).await {
            Ok(()) => {
                // Clean shutdown requested
                info!("Shutting down...");
                break;
            }
            Err(e) => {
                if !reconnect_config.enabled {
                    error!("Connection failed: {}", e);
                    return Err(e);
                }

                let delay = reconnect_config.next_delay();
                warn!("Connection lost: {}. Reconnecting in {:?}...", e, delay);

                // Wait for delay or shutdown signal
                let mut shutdown_rx_clone = shutdown_rx.clone();
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {
                        // Continue to reconnect
                    }
                    _ = shutdown_rx_clone.changed() => {
                        if *shutdown_rx_clone.borrow() {
                            info!("Shutdown requested during reconnect wait");
                            break;
                        }
                    }
                }
            }
        }

        // Check if shutdown was requested
        if *shutdown_rx.borrow() {
            break;
        }

        // Reset backoff on successful connection (will be set in run_connection)
        reconnect_config.reset();
    }

    Ok(())
}

/// Build TLS configuration from args
fn build_tls_config(args: &Args) -> Result<Option<ClientTlsConfig>> {
    if let Some(ca_path) = &args.tls_ca {
        let mut config = rsc_common::tls::ClientTlsConfig::new(ca_path);

        if let Some(server_name) = &args.tls_server_name {
            config = config.with_server_name(server_name);
        }

        if let (Some(cert_path), Some(key_path)) = (&args.tls_cert, &args.tls_key) {
            config = config.with_client_cert(cert_path, key_path);
            info!("mTLS enabled: using client certificate");
        }

        Ok(Some(rsc_common::tls::create_client_tls_config(&config)
            .map_err(|e| ClientError::Tls(e.to_string()))?))
    } else {
        Ok(None)
    }
}

/// Run a single connection lifecycle
/// Returns Ok(()) if shutdown was requested, Err if connection was lost
async fn run_connection(
    server: &str,
    client_id: &str,
    tls_config: Option<ClientTlsConfig>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    // Get reader info (refresh on each connection attempt)
    let reader_info = get_reader_info();

    // Connect to server
    info!("Connecting to server: {}", server);
    let mut client = GrpcClient::connect_with_tls(server, tls_config).await?;
    info!("Connected to server");

    // Establish session
    info!("Establishing session as '{}'", client_id);
    let session_response = client.establish_session(client_id, env!("CARGO_PKG_VERSION")).await?;
    info!("Session established: {}", session_response.session_id);
    info!("Server version: {}", session_response.server_version);

    // Register readers with server
    for (name, atr, card_present) in &reader_info {
        debug!("Registering reader '{}' with server", name);
        if let Err(e) = client.register_reader(name, atr.clone(), *card_present).await {
            warn!("Failed to register reader '{}': {}", name, e);
        }
    }

    // Start command channel for receiving commands from server
    info!("Starting command channel...");
    let mut cmd_shutdown = client.start_command_channel().await?;
    info!("Command channel established - ready to process commands");

    info!("Client ready - waiting for commands from server");
    info!("Press Ctrl+C to exit");

    // Main loop - process commands from server
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30));
    let mut consecutive_heartbeat_failures = 0;
    const MAX_HEARTBEAT_FAILURES: u32 = 3;

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                // Send heartbeat
                match client.heartbeat().await {
                    Ok(response) => {
                        consecutive_heartbeat_failures = 0;
                        if !response.session_valid {
                            warn!("Session invalidated by server");
                            return Err(ClientError::Connection("Session invalidated".to_string()));
                        }
                        debug!("Heartbeat OK");
                    }
                    Err(e) => {
                        consecutive_heartbeat_failures += 1;
                        warn!("Heartbeat failed ({}/{}): {}",
                              consecutive_heartbeat_failures, MAX_HEARTBEAT_FAILURES, e);

                        if consecutive_heartbeat_failures >= MAX_HEARTBEAT_FAILURES {
                            error!("Too many consecutive heartbeat failures, reconnecting...");
                            return Err(ClientError::Connection("Heartbeat failures".to_string()));
                        }
                    }
                }
            }
            _ = cmd_shutdown.recv() => {
                warn!("Command channel closed");
                return Err(ClientError::Connection("Command channel closed".to_string()));
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    // Clean shutdown
                    return Ok(());
                }
            }
        }
    }
}

/// Get information about local smartcard readers
fn get_reader_info() -> Vec<(String, Vec<u8>, bool)> {
    let readers = match pcsc_reader::list_readers() {
        Ok(readers) => {
            if readers.is_empty() {
                warn!("No smartcard readers found locally");
            } else {
                info!("Found {} local reader(s):", readers.len());
                for reader in &readers {
                    info!("  - {}", reader);
                }
            }
            readers
        }
        Err(e) => {
            error!("Failed to list local readers: {}", e);
            vec![]
        }
    };

    let mut reader_info = Vec::new();
    for reader in &readers {
        let atr = pcsc_reader::get_atr(reader).ok();
        let card_present = atr.is_some();
        if let Some(ref atr) = atr {
            info!("Card in '{}': ATR={}", reader, hex::encode(atr));
        }
        reader_info.push((reader.clone(), atr.unwrap_or_default(), card_present));
    }

    reader_info
}
