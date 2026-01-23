//! Remote Smartcard Client
//!
//! Connects to a local smartcard via PC/SC and forwards operations
//! to a remote rsc-server over gRPC with mTLS.

use clap::Parser;
use std::path::PathBuf;
use tracing::{info, error};

mod config;
mod error;
mod pcsc_reader;

use config::ClientConfig;
use error::Result;

/// Remote Smartcard Client
#[derive(Parser, Debug)]
#[command(name = "rsc-client")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/rsc-client/config.yaml")]
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
    let config = match ClientConfig::load(&args.config) {
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

    info!("Starting rsc-client v{}", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded from {:?}", args.config);

    // Test PC/SC connection
    match pcsc_reader::list_readers() {
        Ok(readers) => {
            if readers.is_empty() {
                info!("No smartcard readers found");
            } else {
                info!("Found {} reader(s):", readers.len());
                for reader in &readers {
                    info!("  - {}", reader);
                }

                // Try to read ATR from first reader with card
                for reader in &readers {
                    match pcsc_reader::get_atr(reader) {
                        Ok(atr) => {
                            info!("Card ATR from '{}': {}", reader, hex::encode(&atr));
                        }
                        Err(e) => {
                            info!("No card in '{}': {}", reader, e);
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to list readers: {}", e);
        }
    }

    // TODO: Connect to server
    // TODO: Start forwarding loop

    info!("rsc-client started successfully");
    info!("Server: {}:{}", config.server.host, config.server.port);

    // For now, just wait
    info!("Press Ctrl+C to exit");
    tokio::signal::ctrl_c().await?;

    info!("Shutting down...");
    Ok(())
}
