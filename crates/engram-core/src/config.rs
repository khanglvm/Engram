//! Configuration for the Engram daemon.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Unix socket path for IPC
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,

    /// Data directory for project storage
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Maximum memory usage in bytes (default: 100MB)
    #[serde(default = "default_max_memory")]
    pub max_memory: usize,

    /// Maximum projects to keep in LRU cache
    #[serde(default = "default_max_projects")]
    pub max_projects: usize,

    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// PID file path
    #[serde(default = "default_pid_file")]
    pub pid_file: PathBuf,

    /// Auto-initialize new projects on detection
    #[serde(default)]
    pub auto_init: AutoInitConfig,
}

/// Auto-initialization configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoInitConfig {
    /// Enable auto-initialization
    #[serde(default)]
    pub enabled: bool,

    /// Minimum file count to trigger auto-init
    #[serde(default = "default_min_files")]
    pub min_files: usize,

    /// Patterns to exclude from auto-init consideration
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

fn default_socket_path() -> PathBuf {
    PathBuf::from("/tmp/engram.sock")
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".engram")
}

fn default_max_memory() -> usize {
    100 * 1024 * 1024 // 100MB
}

fn default_max_projects() -> usize {
    3
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_pid_file() -> PathBuf {
    PathBuf::from("/tmp/engram.pid")
}

fn default_min_files() -> usize {
    10
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            data_dir: default_data_dir(),
            max_memory: default_max_memory(),
            max_projects: default_max_projects(),
            log_level: default_log_level(),
            pid_file: default_pid_file(),
            auto_init: AutoInitConfig::default(),
        }
    }
}

impl DaemonConfig {
    /// Load configuration from file, falling back to defaults
    pub fn load() -> Self {
        let config_path = default_data_dir().join("config.yaml");

        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => match serde_yaml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        tracing::warn!("Failed to parse config file: {}", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read config file: {}", e);
                }
            }
        }

        Self::default()
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &PathBuf) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Get the projects directory
    pub fn projects_dir(&self) -> PathBuf {
        self.data_dir.join("projects")
    }

    /// Ensure data directories exist
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.projects_dir())?;
        Ok(())
    }
}

// Need serde_yaml for config loading
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DaemonConfig::default();
        assert_eq!(config.socket_path, PathBuf::from("/tmp/engram.sock"));
        assert_eq!(config.max_memory, 100 * 1024 * 1024);
        assert_eq!(config.max_projects, 3);
    }

    #[test]
    fn test_config_serialization() {
        let config = DaemonConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: DaemonConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.socket_path, parsed.socket_path);
    }
}
