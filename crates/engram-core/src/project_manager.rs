//! Project Manager with LRU caching.
//!
//! Manages loaded projects with efficient memory usage through an LRU cache.

use crate::{CoreError, DaemonConfig, Project};
use lru::LruCache;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages project loading and caching
pub struct ProjectManager {
    /// LRU cache of loaded projects
    projects: RwLock<LruCache<PathBuf, Arc<Project>>>,

    /// Data directory for project storage
    data_dir: PathBuf,

    /// Maximum projects in cache
    max_projects: usize,
}

impl ProjectManager {
    /// Create a new project manager
    pub fn new(config: &DaemonConfig) -> Self {
        let capacity = NonZeroUsize::new(config.max_projects).unwrap_or(NonZeroUsize::MIN);

        Self {
            projects: RwLock::new(LruCache::new(capacity)),
            data_dir: config.data_dir.clone(),
            max_projects: config.max_projects,
        }
    }

    /// Check if a project is initialized
    pub async fn is_initialized(&self, cwd: &Path) -> bool {
        let canonical = match cwd.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };
        let hash = Self::compute_hash(&canonical);
        let manifest_path = self.project_storage_dir(&hash).join("manifest.json");
        manifest_path.exists()
    }

    /// Get a project, loading from disk if not in cache
    pub async fn get_project(&self, cwd: &Path) -> Result<Arc<Project>, CoreError> {
        let canonical = cwd
            .canonicalize()
            .map_err(|_| CoreError::InvalidPath(cwd.display().to_string()))?;

        // Check cache first
        {
            let mut cache = self.projects.write().await;
            if let Some(project) = cache.get(&canonical) {
                return Ok(project.clone());
            }
        }

        // Load from disk
        let project = self.load_project(&canonical).await?;
        let project = Arc::new(project);

        // Add to cache
        {
            let mut cache = self.projects.write().await;
            cache.put(canonical, project.clone());
        }

        Ok(project)
    }

    /// Initialize a new project
    pub async fn init_project(&self, cwd: &Path) -> Result<Arc<Project>, CoreError> {
        let canonical = cwd
            .canonicalize()
            .map_err(|_| CoreError::InvalidPath(cwd.display().to_string()))?;

        let hash = Self::compute_hash(&canonical);
        let storage_dir = self.project_storage_dir(&hash);

        // Check if already initialized
        if storage_dir.join("manifest.json").exists() {
            return Err(CoreError::AlreadyInitialized(
                canonical.display().to_string(),
            ));
        }

        // Create new project
        let project = Project::create(&canonical, &storage_dir, &hash).await?;
        let project = Arc::new(project);

        // Add to cache
        {
            let mut cache = self.projects.write().await;
            cache.put(canonical, project.clone());
        }

        tracing::info!(
            project = ?project.path,
            hash = %hash,
            "Project initialized"
        );

        Ok(project)
    }

    /// Get the number of loaded projects
    pub async fn loaded_count(&self) -> usize {
        self.projects.read().await.len()
    }

    /// Evict the least recently used project from cache
    pub async fn evict_lru(&self) {
        let mut cache = self.projects.write().await;
        if let Some((path, _)) = cache.pop_lru() {
            tracing::debug!(path = ?path, "Evicted project from cache");
        }
    }

    /// Evict all projects except the given one
    pub async fn evict_all_except(&self, keep: &Path) {
        let canonical = keep.canonicalize().ok();
        let mut cache = self.projects.write().await;

        // Collect keys to remove
        let to_remove: Vec<_> = cache
            .iter()
            .filter(|(k, _)| canonical.as_ref() != Some(*k))
            .map(|(k, _)| k.clone())
            .collect();

        for key in to_remove {
            cache.pop(&key);
        }

        tracing::info!("Evicted all projects except current");
    }

    /// Compute a hash for a project path
    fn compute_hash(path: &Path) -> String {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Get the storage directory for a project hash
    fn project_storage_dir(&self, hash: &str) -> PathBuf {
        self.data_dir.join("projects").join(hash)
    }

    /// Load a project from disk
    async fn load_project(&self, cwd: &Path) -> Result<Project, CoreError> {
        let hash = Self::compute_hash(cwd);
        let storage_dir = self.project_storage_dir(&hash);

        if !storage_dir.exists() {
            return Err(CoreError::NotInitialized(cwd.display().to_string()));
        }

        Project::load(&storage_dir).await
    }

    /// Get the tree for a project
    pub async fn get_tree(&self, cwd: &Path) -> Result<engram_indexer::tree::Tree, CoreError> {
        let project = self.get_project(cwd).await?;
        let storage = engram_indexer::storage::Storage::new(self.data_dir.clone());
        storage.load_tree(&project.path, false).await.map_err(|e| {
            CoreError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config(temp_dir: &Path) -> DaemonConfig {
        DaemonConfig {
            data_dir: temp_dir.to_path_buf(),
            max_projects: 3,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_project_manager_init() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        // Create a test project directory
        let project_dir = temp_dir.path().join("test_project");
        std::fs::create_dir_all(&project_dir).unwrap();

        // Check not initialized
        assert!(!manager.is_initialized(&project_dir).await);

        // Initialize
        let project = manager.init_project(&project_dir).await.unwrap();
        assert_eq!(project.manifest.name, "test_project");

        // Check initialized
        assert!(manager.is_initialized(&project_dir).await);

        // Get should return cached
        let cached = manager.get_project(&project_dir).await.unwrap();
        assert_eq!(cached.hash, project.hash);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        // Create 4 projects (cache size is 3)
        for i in 0..4 {
            let project_dir = temp_dir.path().join(format!("project_{}", i));
            std::fs::create_dir_all(&project_dir).unwrap();
            manager.init_project(&project_dir).await.unwrap();
        }

        // Should only have 3 in cache (LRU evicts oldest)
        assert_eq!(manager.loaded_count().await, 3);
    }

    #[tokio::test]
    async fn test_get_project_not_initialized() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        // Create directory but don't init
        let project_dir = temp_dir.path().join("uninitialized");
        std::fs::create_dir_all(&project_dir).unwrap();

        // Get should fail with NotInitialized
        let result = manager.get_project(&project_dir).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_init_already_initialized() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        let project_dir = temp_dir.path().join("test_project");
        std::fs::create_dir_all(&project_dir).unwrap();

        // First init should succeed
        manager.init_project(&project_dir).await.unwrap();

        // Second init should fail
        let result = manager.init_project(&project_dir).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_is_initialized_nonexistent_path() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        // Path that doesn't exist should return false (not error)
        let nonexistent = temp_dir.path().join("nonexistent");
        assert!(!manager.is_initialized(&nonexistent).await);
    }

    #[tokio::test]
    async fn test_evict_all_except() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        // Create 3 projects
        let mut project_dirs = Vec::new();
        for i in 0..3 {
            let project_dir = temp_dir.path().join(format!("project_{}", i));
            std::fs::create_dir_all(&project_dir).unwrap();
            manager.init_project(&project_dir).await.unwrap();
            project_dirs.push(project_dir);
        }

        assert_eq!(manager.loaded_count().await, 3);

        // Evict all except the first
        manager.evict_all_except(&project_dirs[0]).await;

        // Should only have 1 left in cache
        assert_eq!(manager.loaded_count().await, 1);
    }

    #[tokio::test]
    async fn test_evict_lru() {
        let temp_dir = tempdir().unwrap();
        let config = test_config(temp_dir.path());
        let manager = ProjectManager::new(&config);

        // Create 2 projects
        for i in 0..2 {
            let project_dir = temp_dir.path().join(format!("project_{}", i));
            std::fs::create_dir_all(&project_dir).unwrap();
            manager.init_project(&project_dir).await.unwrap();
        }

        assert_eq!(manager.loaded_count().await, 2);

        // Evict LRU
        manager.evict_lru().await;

        assert_eq!(manager.loaded_count().await, 1);
    }
}
