//! Error types for context management.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during context operations.
#[derive(Error, Debug)]
pub enum ContextError {
    /// Scope not found
    #[error("Scope not found: {0}")]
    ScopeNotFound(String),

    /// Project not found
    #[error("Project not found: {0}")]
    ProjectNotFound(PathBuf),

    /// Node not found in tree
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Indexer error
    #[error("Indexer error: {0}")]
    Indexer(#[from] engram_indexer::IndexerError),

    /// Render error
    #[error("Render error: {0}")]
    Render(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ContextError>;
