//! Signal handling for graceful shutdown.

use tokio::sync::broadcast;

/// Wait for shutdown signal (Ctrl+C or explicit shutdown)
pub async fn wait_for_shutdown(mut shutdown_rx: broadcast::Receiver<()>) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT");
        }
        _ = shutdown_rx.recv() => {
            tracing::info!("Received shutdown command");
        }
        _ = wait_for_sigterm() => {
            tracing::info!("Received SIGTERM");
        }
    }
}

/// Wait for SIGTERM signal
#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");

    sigterm.recv().await;
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    // On non-Unix platforms, just wait forever
    std::future::pending::<()>().await;
}
