//! Engram Indexer
//!
//! This crate provides the indexing engine for Engram, including:
//! - Fast file system scanning with gitignore support
//! - AST parsing via tree-sitter for multiple languages
//! - Tree structure building and dependency tracking
//! - Persistence with memory-mapped file access
//! - File watching with debounced incremental updates

mod error;
pub mod scanner;
pub mod storage;
pub mod tree;
pub mod watcher;

pub use error::IndexerError;
pub use scanner::{Language, ScanOptions, ScanResult, ScannedFile, Scanner};
pub use storage::{ExperienceLog, SnapshotManager, Storage, StorageOptions};
pub use tree::{DependencyGraph, Node, NodeId, NodeKind, Tree, TreeBuilder};
pub use watcher::{ChangeBatcher, ChangeKind, FileChange, FileWatcher, WatcherOptions};
