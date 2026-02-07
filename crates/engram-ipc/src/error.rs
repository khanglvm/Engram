//! IPC Error types

use thiserror::Error;

/// Errors that can occur during IPC operations
#[derive(Debug, Error)]
pub enum IpcError {
    /// IO error during socket operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Request size exceeded maximum
    #[error("Request too large (max 1MB)")]
    RequestTooLarge,

    /// Failed to deserialize message
    #[error("Deserialization failed: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),

    /// Failed to serialize message
    #[error("Serialization failed: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),

    /// Request timed out
    #[error("Request timed out")]
    Timeout(#[from] tokio::time::error::Elapsed),

    /// Connection failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Daemon not running
    #[error("Daemon not running (socket not found)")]
    DaemonNotRunning,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: IpcError = io_err.into();
        let msg = format!("{}", err);
        assert!(msg.contains("IO error"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn test_error_display_request_too_large() {
        let err = IpcError::RequestTooLarge;
        let msg = format!("{}", err);
        assert!(msg.contains("too large"));
        assert!(msg.contains("1MB"));
    }

    #[test]
    fn test_error_display_connection_failed() {
        let err = IpcError::ConnectionFailed("test reason".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Connection failed"));
        assert!(msg.contains("test reason"));
    }

    #[test]
    fn test_error_display_daemon_not_running() {
        let err = IpcError::DaemonNotRunning;
        let msg = format!("{}", err);
        assert!(msg.contains("Daemon not running"));
        assert!(msg.contains("socket"));
    }
}
