//! Project data structure and persistence.

use crate::CoreError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Represents an initialized TreeRAG project
#[derive(Debug, Clone)]
pub struct Project {
    /// Absolute path to the project root
    pub path: PathBuf,

    /// Project hash (for storage lookup)
    pub hash: String,

    /// Project manifest
    pub manifest: ProjectManifest,

    /// Storage directory for this project
    pub storage_dir: PathBuf,
}

/// Project manifest stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    /// Version of the manifest format
    pub version: u32,

    /// Absolute path to the project
    pub project_path: PathBuf,

    /// Project name (directory name or configured name)
    pub name: String,

    /// When the project was first initialized
    pub created_at: DateTime<Utc>,

    /// When the project was last scanned
    pub last_scan: Option<DateTime<Utc>>,

    /// Number of files in the project
    pub file_count: usize,

    /// Languages detected in the project
    #[serde(default)]
    pub languages: Vec<String>,

    /// Frameworks detected in the project
    #[serde(default)]
    pub frameworks: Vec<String>,

    /// Whether AI enrichment has completed
    #[serde(default)]
    pub enriched: bool,
}

impl Project {
    /// Load a project from its storage directory
    pub async fn load(storage_dir: &Path) -> Result<Self, CoreError> {
        let manifest_path = storage_dir.join("manifest.json");

        if !manifest_path.exists() {
            return Err(CoreError::NotInitialized(storage_dir.display().to_string()));
        }

        let manifest_content = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest: ProjectManifest = serde_json::from_str(&manifest_content)
            .map_err(|e| CoreError::Serialization(e.to_string()))?;

        // Extract hash from storage directory name
        let hash = storage_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Self {
            path: manifest.project_path.clone(),
            hash,
            manifest,
            storage_dir: storage_dir.to_path_buf(),
        })
    }

    /// Create a new project with initial manifest
    pub async fn create(
        project_path: &Path,
        storage_dir: &Path,
        hash: &str,
    ) -> Result<Self, CoreError> {
        // Ensure storage directory exists
        tokio::fs::create_dir_all(storage_dir).await?;

        let name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let manifest = ProjectManifest {
            version: 1,
            project_path: project_path.to_path_buf(),
            name,
            created_at: Utc::now(),
            last_scan: None,
            file_count: 0,
            languages: vec![],
            frameworks: vec![],
            enriched: false,
        };

        let project = Self {
            path: project_path.to_path_buf(),
            hash: hash.to_string(),
            manifest,
            storage_dir: storage_dir.to_path_buf(),
        };

        project.save_manifest().await?;

        Ok(project)
    }

    /// Save the manifest to disk
    pub async fn save_manifest(&self) -> Result<(), CoreError> {
        let manifest_path = self.storage_dir.join("manifest.json");
        let content = serde_json::to_string_pretty(&self.manifest)
            .map_err(|e| CoreError::Serialization(e.to_string()))?;
        tokio::fs::write(&manifest_path, content).await?;
        Ok(())
    }

    /// Update scan results
    pub async fn update_scan(
        &mut self,
        file_count: usize,
        languages: Vec<String>,
        frameworks: Vec<String>,
    ) -> Result<(), CoreError> {
        self.manifest.last_scan = Some(Utc::now());
        self.manifest.file_count = file_count;
        self.manifest.languages = languages;
        self.manifest.frameworks = frameworks;
        self.save_manifest().await
    }

    /// Mark project as enriched
    pub async fn mark_enriched(&mut self) -> Result<(), CoreError> {
        self.manifest.enriched = true;
        self.save_manifest().await
    }

    /// Get the tree storage path
    pub fn tree_path(&self) -> PathBuf {
        self.storage_dir.join("tree.mmap")
    }

    /// Get the experience log path
    pub fn experience_path(&self) -> PathBuf {
        self.storage_dir.join("experience.jsonl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_project_create_and_load() {
        let temp_dir = tempdir().unwrap();
        let project_path = PathBuf::from("/test/project");
        let storage_dir = temp_dir.path().join("storage");
        let hash = "abc123";

        // Create project
        let project = Project::create(&project_path, &storage_dir, hash)
            .await
            .unwrap();

        assert_eq!(project.path, project_path);
        assert_eq!(project.hash, hash);
        assert_eq!(project.manifest.name, "project");

        // Load project
        let loaded = Project::load(&storage_dir).await.unwrap();
        assert_eq!(loaded.path, project_path);
        assert_eq!(loaded.manifest.name, "project");
    }
}
