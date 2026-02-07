//! Engram Core Components
//!
//! This crate provides the core functionality for the Engram daemon,
//! including project management, configuration, and storage.

mod config;
mod error;
mod metrics;
mod project;
mod project_manager;

pub use config::DaemonConfig;
pub use error::CoreError;
pub use metrics::{LatencyTracker, MemoryMonitor, MemoryPressure, Metrics};
pub use project::Project;
pub use project_manager::ProjectManager;
