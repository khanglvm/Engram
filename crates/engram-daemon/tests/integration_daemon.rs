//! Integration tests for Engram daemon lifecycle and project management.

use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::timeout;

use engram_core::{DaemonConfig, ProjectManager};

/// Helper to create a test config
fn test_config(temp_dir: &std::path::Path) -> DaemonConfig {
    DaemonConfig {
        socket_path: PathBuf::from("/tmp/test.sock"),
        data_dir: temp_dir.to_path_buf(),
        max_memory: 100 * 1024 * 1024,
        max_projects: 5,
        log_level: "debug".to_string(),
        pid_file: temp_dir.join("test.pid"),
        auto_init: Default::default(),
    }
}

/// Test that ProjectManager can be created and configured correctly
#[tokio::test]
async fn test_project_manager_creation() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path());

    let manager = ProjectManager::new(&config);
    assert_eq!(manager.loaded_count().await, 0);
}

/// Test project initialization and loading
#[tokio::test]
async fn test_project_lifecycle_init_and_load() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path().join("test_project");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a simple file structure
    std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(project_dir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

    let config = test_config(temp_dir.path());
    let manager = ProjectManager::new(&config);

    // Initially not initialized
    assert!(!manager.is_initialized(&project_dir).await);

    // Initialize project
    let result = manager.init_project(&project_dir).await;
    assert!(result.is_ok());

    // Now should be initialized
    assert!(manager.is_initialized(&project_dir).await);
    assert_eq!(manager.loaded_count().await, 1);
}

/// Test project eviction (LRU)
#[tokio::test]
async fn test_project_eviction_lru() {
    let temp_dir = tempdir().unwrap();
    let mut config = test_config(temp_dir.path());
    config.max_projects = 2; // Small cache

    let manager = ProjectManager::new(&config);

    // Create and init 3 projects (exceeds cache size)
    for i in 0..3 {
        let project_dir = temp_dir.path().join(format!("project_{}", i));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();
        let _ = manager.init_project(&project_dir).await;
    }

    // Should only have 2 projects loaded (LRU eviction)
    assert!(manager.loaded_count().await <= 2);
}

/// Test evict_all_except functionality
#[tokio::test]
async fn test_evict_all_except() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path());

    let manager = ProjectManager::new(&config);

    // Create and init multiple projects
    let mut project_paths = Vec::new();
    for i in 0..3 {
        let project_dir = temp_dir.path().join(format!("project_{}", i));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();
        let _ = manager.init_project(&project_dir).await;
        project_paths.push(project_dir);
    }

    assert_eq!(manager.loaded_count().await, 3);

    // Evict all except the first project
    manager.evict_all_except(&project_paths[0]).await;

    // Should only have 1 project loaded
    assert_eq!(manager.loaded_count().await, 1);
}

/// Test concurrent project access
#[tokio::test]
async fn test_concurrent_project_access() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path().join("concurrent_project");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();

    let config = test_config(temp_dir.path());
    let manager = std::sync::Arc::new(ProjectManager::new(&config));

    // Initialize project
    manager.init_project(&project_dir).await.unwrap();

    // Spawn multiple concurrent accesses
    let mut handles = Vec::new();
    for _ in 0..10 {
        let mgr = manager.clone();
        let path = project_dir.clone();
        handles.push(tokio::spawn(async move { mgr.is_initialized(&path).await }));
    }

    // All should succeed
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result);
    }
}

/// Test timeout behavior for operations
#[tokio::test]
async fn test_operation_timeout() {
    let result = timeout(Duration::from_millis(100), async {
        // Quick operation should complete
        tokio::time::sleep(Duration::from_millis(10)).await;
        true
    })
    .await;

    assert!(result.is_ok());
    assert!(result.unwrap());
}

/// Test project manager reload from disk
#[tokio::test]
async fn test_project_reload_from_disk() {
    let temp_dir = tempdir().unwrap();
    let project_dir = temp_dir.path().join("reload_project");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();

    let config = test_config(temp_dir.path());

    // First manager - initialize
    {
        let manager = ProjectManager::new(&config);
        manager.init_project(&project_dir).await.unwrap();
        assert!(manager.is_initialized(&project_dir).await);
    }

    // Second manager - should detect existing project
    {
        let manager = ProjectManager::new(&config);
        // Check if can detect previously initialized project
        let is_init = manager.is_initialized(&project_dir).await;
        assert!(is_init);
    }
}

/// Test evict_lru single eviction
#[tokio::test]
async fn test_evict_lru_single() {
    let temp_dir = tempdir().unwrap();
    let config = test_config(temp_dir.path());

    let manager = ProjectManager::new(&config);

    // Create and init 2 projects
    for i in 0..2 {
        let project_dir = temp_dir.path().join(format!("project_{}", i));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();
        let _ = manager.init_project(&project_dir).await;
    }

    assert_eq!(manager.loaded_count().await, 2);

    // Evict LRU
    manager.evict_lru().await;

    // Should have 1 less
    assert_eq!(manager.loaded_count().await, 1);
}
