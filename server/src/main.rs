//! Remote Smartcard Server
//!
//! Receives smartcard operations from rsc-client and forwards them
//! to a virtual smartcard reader (vpcd).

use clap::Parser;
use std::path::PathBuf;
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::info;

mod config;
mod error;
mod grpc_service;
mod session_manager;

use config::ServerConfig;
use error::Result;
use grpc_service::SmartcardService;

/// Remote Smartcard Server
#[derive(Parser, Debug)]
#[command(name = "rsc-server")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override log level
    #[arg(short, long)]
    log_level: Option<String>,

    /// Port to listen on (overrides config)
    #[arg(short, long, default_value = "8443")]
    port: u16,

    /// Address to bind to (overrides config)
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize basic logging first
    let log_level = args.log_level.clone().unwrap_or_else(|| "info".to_string());
    let log_config = rsc_common::config::LoggingConfig {
        level: log_level,
        file: None,
        stdout: true,
        json: false,
    };
    rsc_common::logging::init_logging(&log_config)?;

    info!("Starting rsc-server v{}", env!("CARGO_PKG_VERSION"));

    // Create the gRPC service
    let service = SmartcardService::new();

    // Parse the address
    let addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .map_err(|e| error::ServerError::Config(format!("Invalid address: {}", e)))?;

    info!("Starting gRPC server on {}", addr);

    // Start the server
    Server::builder()
        .add_service(service.into_server())
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c().await.ok();
            info!("Received shutdown signal");
        })
        .await
        .map_err(|e| error::ServerError::Connection(format!("Server error: {}", e)))?;

    info!("Server shut down");
    Ok(())
}
