//! Core error types for Engram.

use thiserror::Error;

/// Errors that can occur in core operations
#[derive(Debug, Error)]
pub enum CoreError {
    /// Project not initialized
    #[error("Project not initialized: {0}")]
    NotInitialized(String),

    /// Project already initialized
    #[error("Project already initialized: {0}")]
    AlreadyInitialized(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid project path
    #[error("Invalid project path: {0}")]
    InvalidPath(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),
}
