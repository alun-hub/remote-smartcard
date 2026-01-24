//! Remote Smartcard Client
//!
//! Connects to a local smartcard via PC/SC and forwards operations
//! to a remote rsc-server over gRPC with mTLS.

use clap::Parser;
use std::path::PathBuf;
use tracing::{info, error, warn, debug};

mod config;
mod error;
mod grpc_client;
mod pcsc_reader;

use error::Result;
use grpc_client::GrpcClient;

/// Remote Smartcard Client
#[derive(Parser, Debug)]
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
    let client_id = args.client_id.unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown-client".to_string())
    });

    // List local smartcard readers
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

    // Collect reader info with ATR
    let mut reader_info = Vec::new();
    for reader in &readers {
        let atr = pcsc_reader::get_atr(reader).ok();
        let card_present = atr.is_some();
        if let Some(ref atr) = atr {
            info!("Card in '{}': ATR={}", reader, hex::encode(atr));
        }
        reader_info.push((reader.clone(), atr.unwrap_or_default(), card_present));
    }

    // Build TLS configuration if CA is provided
    let tls_config = if let Some(ca_path) = &args.tls_ca {
        let mut config = rsc_common::tls::ClientTlsConfig::new(ca_path);

        if let Some(server_name) = &args.tls_server_name {
            config = config.with_server_name(server_name);
        }

        if let (Some(cert_path), Some(key_path)) = (&args.tls_cert, &args.tls_key) {
            config = config.with_client_cert(cert_path, key_path);
            info!("mTLS enabled: using client certificate");
        }

        Some(rsc_common::tls::create_client_tls_config(&config)
            .map_err(|e| error::ClientError::Tls(e.to_string()))?)
    } else {
        None
    };

    // Connect to server
    info!("Connecting to server: {}", args.server);
    let mut client = match GrpcClient::connect_with_tls(&args.server, tls_config).await {
        Ok(c) => {
            info!("Connected to server");
            c
        }
        Err(e) => {
            error!("Failed to connect to server: {}", e);
            return Err(e);
        }
    };

    // Establish session
    info!("Establishing session as '{}'", client_id);
    match client.establish_session(&client_id, env!("CARGO_PKG_VERSION")).await {
        Ok(response) => {
            info!("Session established: {}", response.session_id);
            info!("Server version: {}", response.server_version);
        }
        Err(e) => {
            error!("Failed to establish session: {}", e);
            return Err(e);
        }
    }

    // Register readers with server
    for (name, atr, card_present) in &reader_info {
        debug!("Registering reader '{}' with server", name);
        if let Err(e) = client.register_reader(name, atr.clone(), *card_present).await {
            warn!("Failed to register reader '{}': {}", name, e);
        }
    }

    // Start command channel for receiving commands from server
    info!("Starting command channel...");
    let mut cmd_shutdown = match client.start_command_channel().await {
        Ok(rx) => {
            info!("Command channel established - ready to process commands");
            rx
        }
        Err(e) => {
            error!("Failed to start command channel: {}", e);
            return Err(e);
        }
    };

    info!("Client ready - waiting for commands from server");
    info!("Press Ctrl+C to exit");

    // Main loop - process commands from server
    let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                // Send heartbeat
                match client.heartbeat().await {
                    Ok(response) => {
                        if !response.session_valid {
                            warn!("Session invalidated by server, reconnecting...");
                            // TODO: Implement reconnect
                            break;
                        }
                        debug!("Heartbeat OK");
                    }
                    Err(e) => {
                        error!("Heartbeat failed: {}", e);
                        // TODO: Implement reconnect
                    }
                }
            }
            _ = cmd_shutdown.recv() => {
                warn!("Command channel closed");
                // TODO: Implement reconnect
                break;
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
                break;
            }
        }
    }

    info!("Shutting down...");
    Ok(())
}
