//! Remote Smartcard Server
//!
//! Receives smartcard operations from rsc-client and forwards them
//! to a virtual smartcard reader (vpcd).

use clap::Parser;
use std::path::PathBuf;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Server;
use tracing::info;

mod config;
mod error;
mod grpc_service;
mod session_manager;
mod vpcd;

use error::Result;
use grpc_service::SmartcardService;
use session_manager::SessionManager;
use vpcd::VpcdManager;

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
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// vpcd host to connect to
    #[arg(long, default_value = "127.0.0.1")]
    vpcd_host: String,

    /// vpcd port to connect to
    #[arg(long, default_value = "35963")]
    vpcd_port: u16,

    /// Automatically connect to vpcd when a reader is registered
    #[arg(long)]
    auto_vpcd: bool,

    // TLS options
    /// Server certificate file (PEM format) - enables TLS if provided
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// Server private key file (PEM format)
    #[arg(long)]
    tls_key: Option<PathBuf>,

    /// CA certificate for client verification (enables mTLS)
    #[arg(long)]
    tls_ca: Option<PathBuf>,

    /// Require client certificate (mTLS)
    #[arg(long)]
    tls_require_client_cert: bool,
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

    // Create shared session manager
    let sessions = Arc::new(RwLock::new(SessionManager::new()));

    // Create vpcd manager
    let vpcd_manager = Arc::new(VpcdManager::new(sessions.clone()));

    // Create the gRPC service with shared session manager
    let service = SmartcardService::with_sessions(
        sessions.clone(),
        vpcd_manager.clone(),
        args.vpcd_host.clone(),
        args.vpcd_port,
        args.auto_vpcd,
    );

    // Parse the address
    let addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .map_err(|e| error::ServerError::Config(format!("Invalid address: {}", e)))?;

    if args.auto_vpcd {
        info!("Auto-vpcd enabled: will connect to {}:{} for each reader", args.vpcd_host, args.vpcd_port);
    }

    // Configure TLS if certificates are provided
    let tls_config = if let (Some(cert_path), Some(key_path)) = (&args.tls_cert, &args.tls_key) {
        let mut config = rsc_common::tls::ServerTlsConfig::new(cert_path, key_path);

        if let Some(ca_path) = &args.tls_ca {
            config = config.with_client_auth(ca_path, args.tls_require_client_cert);
            if args.tls_require_client_cert {
                info!("mTLS enabled: client certificates required");
            } else {
                info!("mTLS enabled: client certificates optional");
            }
        }

        Some(rsc_common::tls::create_server_tls_config(&config)
            .map_err(|e| error::ServerError::Tls(e.to_string()))?)
    } else {
        None
    };

    // Start the server
    if let Some(tls) = tls_config {
        info!("Starting gRPC server on {} (TLS enabled)", addr);

        Server::builder()
            .tls_config(tls)
            .map_err(|e| error::ServerError::Tls(format!("TLS config error: {}", e)))?
            .add_service(service.into_server())
            .serve_with_shutdown(addr, async {
                tokio::signal::ctrl_c().await.ok();
                info!("Received shutdown signal");
            })
            .await
            .map_err(|e| error::ServerError::Connection(format!("Server error: {}", e)))?;
    } else {
        info!("Starting gRPC server on {} (no TLS - insecure)", addr);
        info!("WARNING: Use --tls-cert and --tls-key for secure connections");

        Server::builder()
            .add_service(service.into_server())
            .serve_with_shutdown(addr, async {
                tokio::signal::ctrl_c().await.ok();
                info!("Received shutdown signal");
            })
            .await
            .map_err(|e| error::ServerError::Connection(format!("Server error: {}", e)))?;
    }

    info!("Server shut down");
    Ok(())
}
