//! Logging initialization utilities

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::LoggingConfig;
use crate::Result;

/// Initialize logging based on configuration
pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let subscriber = tracing_subscriber::registry().with(filter);

    if config.json {
        let layer = fmt::layer().json();
        subscriber.with(layer).init();
    } else {
        let layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(false);
        subscriber.with(layer).init();
    }

    Ok(())
}
