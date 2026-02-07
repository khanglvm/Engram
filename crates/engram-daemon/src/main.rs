//! Engram Daemon
//!
//! Background process that manages project context for AI coding assistants.

mod daemon;
mod handler;
mod signals;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

pub use daemon::Daemon;

/// Run the daemon
pub async fn run() -> Result<()> {
    let daemon = Daemon::new()?;
    daemon.run().await
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tracing::info!("Starting Engram daemon v{}", env!("CARGO_PKG_VERSION"));

    // Run async runtime
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}
