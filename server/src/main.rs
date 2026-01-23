//! Remote Smartcard Server
//!
//! Receives smartcard operations from rsc-client and forwards them
//! to a virtual smartcard reader (vpcd).

use clap::Parser;
use std::path::PathBuf;
use tracing::info;

mod config;
mod error;

use config::ServerConfig;
use error::Result;

/// Remote Smartcard Server
#[derive(Parser, Debug)]
#[command(name = "rsc-server")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/rsc-server/config.yaml")]
    config: PathBuf,

    /// Override log level
    #[arg(short, long)]
    log_level: Option<String>,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config = match ServerConfig::load(&args.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging
    let log_config = if let Some(level) = args.log_level {
        let mut log_config = config.logging.clone();
        log_config.level = level;
        log_config
    } else {
        config.logging.clone()
    };

    rsc_common::logging::init_logging(&log_config)?;

    info!("Starting rsc-server v{}", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded from {:?}", args.config);

    // TODO: Initialize gRPC server
    // TODO: Start listening for connections

    let addr = format!("{}:{}", config.server.bind_address, config.server.port);
    info!("Listening on {}", addr);

    // For now, just wait
    info!("Press Ctrl+C to exit");
    tokio::signal::ctrl_c().await?;

    info!("Shutting down...");
    Ok(())
}
