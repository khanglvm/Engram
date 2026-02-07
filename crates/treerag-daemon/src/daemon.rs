//! Daemon lifecycle management.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use treerag_core::{DaemonConfig, ProjectManager};
use treerag_indexer::storage::Storage;
use treerag_ipc::IpcServer;

use crate::handler::DaemonHandler;
use crate::signals;

/// The main daemon process
pub struct Daemon {
    config: DaemonConfig,
    shutdown_tx: broadcast::Sender<()>,
    is_running: Arc<AtomicBool>,
    start_time: std::time::Instant,
}

impl Daemon {
    /// Create a new daemon instance
    pub fn new() -> Result<Self> {
        let config = DaemonConfig::load();

        // Ensure data directories exist
        config
            .ensure_dirs()
            .context("Failed to create data directories")?;

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            config,
            shutdown_tx,
            is_running: Arc::new(AtomicBool::new(false)),
            start_time: std::time::Instant::now(),
        })
    }

    /// Run the daemon
    pub async fn run(&self) -> Result<()> {
        // Check single instance
        self.acquire_pid_lock()?;

        // Mark as running
        self.is_running.store(true, Ordering::SeqCst);

        tracing::info!(
            socket = %self.config.socket_path.display(),
            data_dir = %self.config.data_dir.display(),
            "Daemon starting"
        );

        // Initialize components
        let project_manager = Arc::new(ProjectManager::new(&self.config));
        let storage = Arc::new(Storage::new(self.config.data_dir.clone()));

        let handler = Arc::new(DaemonHandler::new(
            project_manager.clone(),
            storage,
            self.shutdown_tx.clone(),
            self.start_time,
        ));

        let ipc_server = IpcServer::new(&self.config.socket_path, handler)
            .await
            .context("Failed to create IPC server")?;

        // Set up shutdown signal
        let shutdown_rx = self.shutdown_tx.subscribe();

        // Run components
        tokio::select! {
            result = ipc_server.run() => {
                if let Err(e) = result {
                    tracing::error!("IPC server error: {}", e);
                }
            }
            _ = signals::wait_for_shutdown(shutdown_rx) => {
                tracing::info!("Shutdown signal received");
            }
        }

        // Cleanup
        self.cleanup().await?;

        Ok(())
    }

    /// Acquire PID lock to ensure single instance
    fn acquire_pid_lock(&self) -> Result<()> {
        let pid_file = &self.config.pid_file;

        if pid_file.exists() {
            // Read existing PID
            if let Ok(pid_str) = std::fs::read_to_string(pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    // Check if process is actually running
                    if is_process_running(pid) {
                        anyhow::bail!("Daemon already running (PID: {})", pid);
                    }
                }
            }
            // Stale PID file, remove it
            std::fs::remove_file(pid_file)?;
        }

        // Write our PID
        std::fs::write(pid_file, std::process::id().to_string())?;

        tracing::debug!(pid = std::process::id(), "PID lock acquired");

        Ok(())
    }

    /// Cleanup resources on shutdown
    async fn cleanup(&self) -> Result<()> {
        tracing::info!("Cleaning up...");

        // Remove socket file
        if self.config.socket_path.exists() {
            let _ = std::fs::remove_file(&self.config.socket_path);
        }

        // Remove PID file
        if self.config.pid_file.exists() {
            let _ = std::fs::remove_file(&self.config.pid_file);
        }

        self.is_running.store(false, Ordering::SeqCst);

        tracing::info!("Cleanup complete");

        Ok(())
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        // Ensure cleanup happens even on panic
        if self.config.pid_file.exists() {
            let _ = std::fs::remove_file(&self.config.pid_file);
        }
    }
}

/// Check if a process is running by PID
fn is_process_running(pid: u32) -> bool {
    // Use kill(pid, 0) to check if process exists
    // This doesn't actually send a signal, just checks existence
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_process_running() {
        // Current process should be running
        assert!(is_process_running(std::process::id()));

        // Very high PID should not exist
        assert!(!is_process_running(999999999));
    }
}
