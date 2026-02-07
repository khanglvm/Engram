//! Persistence layer for tree storage.
//!
//! Provides storage operations for saving and loading tree data,
//! including fast skeleton loading and memory-mapped access.

mod experience;
mod snapshot;

pub use experience::ExperienceLog;
pub use snapshot::SnapshotManager;

use crate::tree::Tree;
use crate::IndexerError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Storage options.
#[derive(Debug, Clone)]
pub struct StorageOptions {
    /// Base directory for storage
    pub base_dir: PathBuf,
    /// Whether to use MessagePack for enriched data
    pub use_msgpack: bool,
    /// Maximum experience log size before rotation (bytes)
    pub max_experience_size: u64,
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            base_dir: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("treerag")
                .join("projects"),
            use_msgpack: true,
            max_experience_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Manages storage for project trees.
pub struct Storage {
    options: StorageOptions,
}

impl Storage {
    /// Create a new storage manager with default options.
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            options: StorageOptions {
                base_dir,
                ..Default::default()
            },
        }
    }

    /// Create a storage manager with custom options.
    pub fn with_options(options: StorageOptions) -> Self {
        Self { options }
    }

    /// Compute a hash for a project path.
    pub fn project_hash(&self, project_path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(project_path.to_string_lossy().as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)[..16].to_string()
    }

    /// Get the storage directory for a project hash.
    pub fn project_dir(&self, hash: &str) -> PathBuf {
        self.options.base_dir.join(hash)
    }

    /// Load a tree from storage (skeleton or enriched based on flag).
    pub async fn load_tree(
        &self,
        project_path: &Path,
        enriched: bool,
    ) -> Result<Tree, IndexerError> {
        let hash = self.project_hash(project_path);
        if enriched {
            self.load_enriched(&hash).await
        } else {
            self.load_skeleton(&hash).await
        }
    }

    /// Append an experience to the project's experience log.
    pub async fn append_experience<E: serde::Serialize>(
        &self,
        project_path: &Path,
        experience: &E,
    ) -> Result<(), IndexerError> {
        let hash = self.project_hash(project_path);
        let log = self.experience_log(&hash);

        let json = serde_json::to_string(experience)
            .map_err(|e| IndexerError::Serialization(e.to_string()))?;

        log.append_raw(&json).await
    }

    /// Append an experience with durable fsync semantics.
    pub async fn append_experience_durable<E: serde::Serialize>(
        &self,
        project_path: &Path,
        experience: &E,
    ) -> Result<(), IndexerError> {
        let hash = self.project_hash(project_path);
        let log = self.experience_log(&hash);

        let json = serde_json::to_string(experience)
            .map_err(|e| IndexerError::Serialization(e.to_string()))?;

        log.append_raw_durable(&json).await
    }

    /// Load experiences from the project's experience log.
    pub async fn load_experiences<E: serde::de::DeserializeOwned>(
        &self,
        project_path: &Path,
        limit: usize,
    ) -> Result<Vec<E>, IndexerError> {
        let hash = self.project_hash(project_path);
        let log = self.experience_log(&hash);
        log.read_recent(limit).await
    }

    /// Load all parseable experiences from the log (oldest first).
    pub async fn load_all_experiences<E: serde::de::DeserializeOwned>(
        &self,
        project_path: &Path,
    ) -> Result<Vec<E>, IndexerError> {
        let hash = self.project_hash(project_path);
        let log = self.experience_log(&hash);
        log.read_recent(usize::MAX).await
    }

    /// Save a tree skeleton (structure only, fast).
    pub async fn save_skeleton(&self, tree: &Tree, hash: &str) -> Result<(), IndexerError> {
        let dir = self.project_dir(hash);
        tokio::fs::create_dir_all(&dir).await?;

        let skeleton_path = dir.join("skeleton.json");

        // Create skeleton version (no content/symbols)
        let skeleton = create_skeleton(tree);

        let json = serde_json::to_string_pretty(&skeleton)
            .map_err(|e| IndexerError::Serialization(e.to_string()))?;

        // Atomic write: write to temp file, then rename
        let temp_path = dir.join(".skeleton.json.tmp");
        tokio::fs::write(&temp_path, &json).await?;
        tokio::fs::rename(&temp_path, &skeleton_path).await?;

        debug!(path = ?skeleton_path, size = json.len(), "Saved skeleton");

        Ok(())
    }

    /// Load a tree skeleton (fast initial load).
    pub async fn load_skeleton(&self, hash: &str) -> Result<Tree, IndexerError> {
        let skeleton_path = self.project_dir(hash).join("skeleton.json");

        if !skeleton_path.exists() {
            return Err(IndexerError::NotFound(skeleton_path));
        }

        let json = tokio::fs::read_to_string(&skeleton_path).await?;
        let tree: Tree =
            serde_json::from_str(&json).map_err(|e| IndexerError::Serialization(e.to_string()))?;

        debug!(path = ?skeleton_path, nodes = tree.nodes.len(), "Loaded skeleton");

        Ok(tree)
    }

    /// Save a full enriched tree.
    pub async fn save_enriched(&self, tree: &Tree, hash: &str) -> Result<(), IndexerError> {
        let dir = self.project_dir(hash);
        tokio::fs::create_dir_all(&dir).await?;

        let enriched_path = if self.options.use_msgpack {
            dir.join("enriched.msgpack")
        } else {
            dir.join("enriched.json")
        };

        let data = if self.options.use_msgpack {
            rmp_serde::to_vec(tree).map_err(|e| IndexerError::Serialization(e.to_string()))?
        } else {
            serde_json::to_vec_pretty(tree)
                .map_err(|e| IndexerError::Serialization(e.to_string()))?
        };

        // Atomic write
        let temp_path = dir.join(".enriched.tmp");
        tokio::fs::write(&temp_path, &data).await?;
        tokio::fs::rename(&temp_path, &enriched_path).await?;

        info!(path = ?enriched_path, size = data.len(), "Saved enriched tree");

        Ok(())
    }

    /// Load a full enriched tree.
    pub async fn load_enriched(&self, hash: &str) -> Result<Tree, IndexerError> {
        let dir = self.project_dir(hash);

        // Try MessagePack first, then JSON
        let msgpack_path = dir.join("enriched.msgpack");
        let json_path = dir.join("enriched.json");

        if msgpack_path.exists() {
            let data = tokio::fs::read(&msgpack_path).await?;
            let tree: Tree = rmp_serde::from_slice(&data)
                .map_err(|e| IndexerError::Serialization(e.to_string()))?;
            debug!(path = ?msgpack_path, "Loaded enriched (msgpack)");
            return Ok(tree);
        }

        if json_path.exists() {
            let json = tokio::fs::read_to_string(&json_path).await?;
            let tree: Tree = serde_json::from_str(&json)
                .map_err(|e| IndexerError::Serialization(e.to_string()))?;
            debug!(path = ?json_path, "Loaded enriched (json)");
            return Ok(tree);
        }

        Err(IndexerError::NotFound(dir))
    }

    /// Load tree with memory mapping (lazy access).
    ///
    /// Note: For now, this loads the full tree into memory.
    /// Full mmap implementation would require a custom format with offset tables.
    pub async fn load_tree_mmap(&self, hash: &str) -> Result<Tree, IndexerError> {
        // For initial implementation, just load enriched or skeleton
        if let Ok(tree) = self.load_enriched(hash).await {
            return Ok(tree);
        }

        self.load_skeleton(hash).await
    }

    /// Save dependencies separately (for faster updates).
    pub async fn save_dependencies(&self, tree: &Tree, hash: &str) -> Result<(), IndexerError> {
        let dir = self.project_dir(hash);
        tokio::fs::create_dir_all(&dir).await?;

        let path = dir.join("dependencies.json");
        let json = serde_json::to_string_pretty(&tree.dependencies)
            .map_err(|e| IndexerError::Serialization(e.to_string()))?;

        tokio::fs::write(&path, json).await?;

        Ok(())
    }

    /// Check if a project has stored data.
    pub async fn exists(&self, hash: &str) -> bool {
        let dir = self.project_dir(hash);
        dir.join("skeleton.json").exists() || dir.join("enriched.msgpack").exists()
    }

    /// Delete all stored data for a project.
    pub async fn delete(&self, hash: &str) -> Result<(), IndexerError> {
        let dir = self.project_dir(hash);
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    /// Get an experience log for a project.
    pub fn experience_log(&self, hash: &str) -> ExperienceLog {
        let path = self.project_dir(hash).join("experience.jsonl");
        ExperienceLog::new(path, self.options.max_experience_size)
    }

    /// Get a snapshot manager for a project.
    pub fn snapshots(&self, hash: &str) -> SnapshotManager {
        let dir = self.project_dir(hash).join("snapshots");
        SnapshotManager::new(dir)
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::with_options(StorageOptions::default())
    }
}

/// Create a skeleton version of a tree (no content).
fn create_skeleton(tree: &Tree) -> Tree {
    let mut skeleton = tree.clone();

    // Clear content from all nodes
    for node in skeleton.nodes.values_mut() {
        node.content = None;
    }

    skeleton
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_storage(temp_dir: &Path) -> Storage {
        Storage::with_options(StorageOptions {
            base_dir: temp_dir.to_path_buf(),
            use_msgpack: true,
            max_experience_size: 1024,
        })
    }

    fn test_tree() -> Tree {
        Tree::new(PathBuf::from("/test/project"))
    }

    #[tokio::test]
    async fn test_save_and_load_skeleton() {
        let temp_dir = tempdir().unwrap();
        let storage = test_storage(temp_dir.path());
        let tree = test_tree();
        let hash = "abc123";

        storage.save_skeleton(&tree, hash).await.unwrap();

        let loaded = storage.load_skeleton(hash).await.unwrap();

        assert_eq!(tree.root_path, loaded.root_path);
        assert_eq!(tree.nodes.len(), loaded.nodes.len());
    }

    #[tokio::test]
    async fn test_save_and_load_enriched() {
        let temp_dir = tempdir().unwrap();
        let storage = test_storage(temp_dir.path());
        let tree = test_tree();
        let hash = "def456";

        storage.save_enriched(&tree, hash).await.unwrap();

        let loaded = storage.load_enriched(hash).await.unwrap();

        assert_eq!(tree.root_path, loaded.root_path);
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = tempdir().unwrap();
        let storage = test_storage(temp_dir.path());
        let tree = test_tree();
        let hash = "exists_test";

        assert!(!storage.exists(hash).await);

        storage.save_skeleton(&tree, hash).await.unwrap();

        assert!(storage.exists(hash).await);
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let storage = test_storage(temp_dir.path());
        let tree = test_tree();
        let hash = "delete_test";

        storage.save_skeleton(&tree, hash).await.unwrap();
        assert!(storage.exists(hash).await);

        storage.delete(hash).await.unwrap();
        assert!(!storage.exists(hash).await);
    }

    #[tokio::test]
    async fn test_load_not_found() {
        let temp_dir = tempdir().unwrap();
        let storage = test_storage(temp_dir.path());

        let result = storage.load_skeleton("nonexistent").await;
        assert!(matches!(result, Err(IndexerError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_skeleton_removes_content() {
        let mut tree = test_tree();
        // Add some content
        if let Some(root) = tree.nodes.get_mut(&0) {
            root.content = Some(crate::tree::NodeContent::default());
        }

        let skeleton = create_skeleton(&tree);

        // Content should be cleared
        assert!(skeleton.nodes.get(&0).unwrap().content.is_none());
    }

    #[test]
    fn test_project_dir() {
        let storage = Storage::with_options(StorageOptions {
            base_dir: PathBuf::from("/base"),
            ..Default::default()
        });

        let dir = storage.project_dir("abc123");
        assert_eq!(dir, PathBuf::from("/base/abc123"));
    }

    #[tokio::test]
    async fn test_append_experience_durable_and_load_all() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct Record {
            id: String,
            content: String,
        }

        let temp_dir = tempdir().unwrap();
        let storage = test_storage(temp_dir.path());
        let project = temp_dir.path().join("memory_project");
        std::fs::create_dir_all(&project).unwrap();

        let first = Record {
            id: "1".to_string(),
            content: "first".to_string(),
        };
        let second = Record {
            id: "2".to_string(),
            content: "second".to_string(),
        };

        storage
            .append_experience_durable(&project, &first)
            .await
            .unwrap();
        storage
            .append_experience_durable(&project, &second)
            .await
            .unwrap();

        let loaded: Vec<Record> = storage.load_all_experiences(&project).await.unwrap();
        assert_eq!(loaded, vec![first, second]);
    }
}
