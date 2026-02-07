//! Snapshot management for tree backups.

use crate::IndexerError;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tracing::{debug, info};

/// Manages snapshots of tree data.
pub struct SnapshotManager {
    dir: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Create a snapshot of the current tree data.
    pub async fn create(&self, source_dir: &PathBuf) -> Result<String, IndexerError> {
        tokio::fs::create_dir_all(&self.dir).await?;

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let snapshot_dir = self.dir.join(&timestamp);

        // Copy all files from source to snapshot
        copy_dir_recursive(source_dir, &snapshot_dir).await?;

        info!(snapshot = %timestamp, path = ?snapshot_dir, "Created snapshot");

        Ok(timestamp)
    }

    /// List all available snapshots.
    pub async fn list(&self) -> Result<Vec<SnapshotInfo>, IndexerError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();

                // Parse timestamp from name
                if let Some(timestamp) = parse_snapshot_timestamp(&name) {
                    let metadata = entry.metadata().await?;
                    snapshots.push(SnapshotInfo {
                        name,
                        timestamp,
                        size: calculate_dir_size(&entry.path()).await.unwrap_or(0),
                    });
                }
            }
        }

        // Sort by timestamp descending (newest first)
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(snapshots)
    }

    /// Restore from a snapshot.
    pub async fn restore(&self, name: &str, target_dir: &PathBuf) -> Result<(), IndexerError> {
        let snapshot_dir = self.dir.join(name);

        if !snapshot_dir.exists() {
            return Err(IndexerError::NotFound(snapshot_dir));
        }

        // Clear target directory
        if target_dir.exists() {
            tokio::fs::remove_dir_all(target_dir).await?;
        }

        // Copy snapshot to target
        copy_dir_recursive(&snapshot_dir, target_dir).await?;

        info!(snapshot = %name, target = ?target_dir, "Restored snapshot");

        Ok(())
    }

    /// Delete a snapshot.
    pub async fn delete(&self, name: &str) -> Result<(), IndexerError> {
        let snapshot_dir = self.dir.join(name);

        if snapshot_dir.exists() {
            tokio::fs::remove_dir_all(&snapshot_dir).await?;
            debug!(snapshot = %name, "Deleted snapshot");
        }

        Ok(())
    }

    /// Delete old snapshots, keeping the N most recent.
    pub async fn prune(&self, keep: usize) -> Result<usize, IndexerError> {
        let snapshots = self.list().await?;

        if snapshots.len() <= keep {
            return Ok(0);
        }

        let mut deleted = 0;
        for snapshot in snapshots.into_iter().skip(keep) {
            self.delete(&snapshot.name).await?;
            deleted += 1;
        }

        info!(deleted = deleted, kept = keep, "Pruned snapshots");

        Ok(deleted)
    }
}

/// Information about a snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Snapshot name (timestamp)
    pub name: String,
    /// When the snapshot was created
    pub timestamp: DateTime<Utc>,
    /// Total size in bytes
    pub size: u64,
}

/// Parse a snapshot timestamp from its name.
fn parse_snapshot_timestamp(name: &str) -> Option<DateTime<Utc>> {
    // Format: YYYYMMDD_HHMMSS
    chrono::NaiveDateTime::parse_from_str(name, "%Y%m%d_%H%M%S")
        .ok()
        .map(|dt| dt.and_utc())
}

/// Recursively copy a directory.
async fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<(), IndexerError> {
    tokio::fs::create_dir_all(dst).await?;

    let mut entries = tokio::fs::read_dir(src).await?;

    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_type().await?.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}

/// Calculate the total size of a directory.
async fn calculate_dir_size(path: &PathBuf) -> Result<u64, IndexerError> {
    let mut size = 0;
    let mut entries = tokio::fs::read_dir(path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        if metadata.is_dir() {
            size += Box::pin(calculate_dir_size(&entry.path())).await?;
        } else {
            size += metadata.len();
        }
    }

    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_create_snapshot() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let snapshot_dir = temp_dir.path().join("snapshots");

        // Create source files
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("file1.txt"), "content1").unwrap();
        fs::write(source_dir.join("file2.txt"), "content2").unwrap();

        let manager = SnapshotManager::new(snapshot_dir);
        let name = manager.create(&source_dir).await.unwrap();

        assert!(!name.is_empty());

        let snapshots = manager.list().await.unwrap();
        assert_eq!(snapshots.len(), 1);
    }

    #[tokio::test]
    async fn test_restore_snapshot() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let snapshot_dir = temp_dir.path().join("snapshots");
        let restore_dir = temp_dir.path().join("restored");

        // Create source files
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("test.txt"), "original").unwrap();

        let manager = SnapshotManager::new(snapshot_dir);
        let name = manager.create(&source_dir).await.unwrap();

        // Modify source
        fs::write(source_dir.join("test.txt"), "modified").unwrap();

        // Restore
        manager.restore(&name, &restore_dir).await.unwrap();

        // Check restored content
        let content = fs::read_to_string(restore_dir.join("test.txt")).unwrap();
        assert_eq!(content, "original");
    }

    #[tokio::test]
    async fn test_list_snapshots() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let snapshot_dir = temp_dir.path().join("snapshots");

        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("file.txt"), "content").unwrap();

        let manager = SnapshotManager::new(snapshot_dir);

        // Create multiple snapshots
        manager.create(&source_dir).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        manager.create(&source_dir).await.unwrap();

        let snapshots = manager.list().await.unwrap();
        assert_eq!(snapshots.len(), 2);

        // Should be sorted newest first
        assert!(snapshots[0].timestamp >= snapshots[1].timestamp);
    }

    #[tokio::test]
    async fn test_prune_snapshots() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let snapshot_dir = temp_dir.path().join("snapshots");

        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("file.txt"), "content").unwrap();

        let manager = SnapshotManager::new(snapshot_dir);

        // Create 3 snapshots
        for _ in 0..3 {
            manager.create(&source_dir).await.unwrap();
            tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        }

        // Keep only 1
        let deleted = manager.prune(1).await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = manager.list().await.unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_snapshot() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let snapshot_dir = temp_dir.path().join("snapshots");

        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("file.txt"), "content").unwrap();

        let manager = SnapshotManager::new(snapshot_dir);
        let name = manager.create(&source_dir).await.unwrap();

        manager.delete(&name).await.unwrap();

        let snapshots = manager.list().await.unwrap();
        assert!(snapshots.is_empty());
    }

    #[test]
    fn test_parse_timestamp() {
        let name = "20240115_143052";
        let ts = parse_snapshot_timestamp(name);
        assert!(ts.is_some());

        let invalid = "invalid_name";
        assert!(parse_snapshot_timestamp(invalid).is_none());
    }
}
