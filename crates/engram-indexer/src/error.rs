//! Indexer error types.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during indexing operations.
#[derive(Debug, Error)]
pub enum IndexerError {
    /// I/O error during file operations
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse file with tree-sitter
    #[error("Parse error in {path}: {message}")]
    Parse { path: PathBuf, message: String },

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// File watcher error
    #[error("Watcher error: {0}")]
    Watcher(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Path not found
    #[error("Path not found: {0}")]
    NotFound(PathBuf),

    /// Invalid language
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}

impl From<serde_json::Error> for IndexerError {
    fn from(e: serde_json::Error) -> Self {
        IndexerError::Serialization(e.to_string())
    }
}

impl From<rmp_serde::encode::Error> for IndexerError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        IndexerError::Serialization(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for IndexerError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        IndexerError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = IndexerError::NotFound(PathBuf::from("/test/path"));
        assert!(err.to_string().contains("/test/path"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: IndexerError = io_err.into();
        assert!(matches!(err, IndexerError::Io(_)));
    }
}
