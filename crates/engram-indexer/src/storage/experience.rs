//! Experience log for recording agent decisions.

use crate::IndexerError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::debug;

/// An entry in the experience log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceEntry {
    /// Timestamp of the entry
    pub timestamp: DateTime<Utc>,
    /// Agent that created this entry
    pub agent_id: String,
    /// Action taken
    pub action: String,
    /// Context or prompt that led to the action
    pub context: Option<String>,
    /// Outcome or result
    pub outcome: Option<String>,
    /// Relevance score (for learning)
    pub score: Option<f32>,
}

/// Append-only experience log.
pub struct ExperienceLog {
    path: PathBuf,
    max_size: u64,
}

impl ExperienceLog {
    /// Create a new experience log.
    pub fn new(path: PathBuf, max_size: u64) -> Self {
        Self { path, max_size }
    }

    /// Append an entry to the log.
    pub async fn append(&self, entry: &ExperienceEntry) -> Result<(), IndexerError> {
        let json =
            serde_json::to_string(entry).map_err(|e| IndexerError::Serialization(e.to_string()))?;
        self.append_raw(&json).await
    }

    /// Append a raw JSON string to the log.
    pub async fn append_raw(&self, json: &str) -> Result<(), IndexerError> {
        self.append_raw_inner(json, false).await
    }

    /// Append a raw JSON string and fsync before returning.
    pub async fn append_raw_durable(&self, json: &str) -> Result<(), IndexerError> {
        self.append_raw_inner(json, true).await
    }

    async fn append_raw_inner(&self, json: &str, durable: bool) -> Result<(), IndexerError> {
        // Check if rotation is needed
        if self.should_rotate().await {
            self.rotate().await?;
        }

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Append to file with newline
        let mut line = json.to_string();
        if !line.ends_with('\n') {
            line.push('\n');
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;

        file.write_all(line.as_bytes()).await?;
        file.flush().await?;
        if durable {
            file.sync_all().await?;
        }

        debug!(path = ?self.path, "Appended experience entry");

        Ok(())
    }

    /// Read all entries from the log.
    pub async fn read_all(&self) -> Result<Vec<ExperienceEntry>, IndexerError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&self.path).await?;
        let mut entries = Vec::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: ExperienceEntry = serde_json::from_str(line)
                .map_err(|e| IndexerError::Serialization(e.to_string()))?;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Get the number of entries in the log.
    pub async fn count(&self) -> Result<usize, IndexerError> {
        if !self.path.exists() {
            return Ok(0);
        }

        let content = tokio::fs::read_to_string(&self.path).await?;
        Ok(content.lines().filter(|l| !l.trim().is_empty()).count())
    }

    /// Read recent entries from the log (generic deserialization).
    pub async fn read_recent<E: serde::de::DeserializeOwned>(
        &self,
        limit: usize,
    ) -> Result<Vec<E>, IndexerError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&self.path).await?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

        // Walk backwards so we can return "last N valid entries"
        // even when recent lines include unrelated schemas.
        let mut entries_rev = Vec::new();
        for line in lines.iter().rev() {
            match serde_json::from_str(line) {
                Ok(entry) => {
                    entries_rev.push(entry);
                    if entries_rev.len() >= limit {
                        break;
                    }
                }
                Err(e) => {
                    debug!(error = %e, "Skipping malformed experience entry");
                }
            }
        }

        entries_rev.reverse();
        let entries = entries_rev;

        Ok(entries)
    }

    /// Check if the log needs rotation.
    async fn should_rotate(&self) -> bool {
        if !self.path.exists() {
            return false;
        }

        match tokio::fs::metadata(&self.path).await {
            Ok(meta) => meta.len() >= self.max_size,
            Err(_) => false,
        }
    }

    /// Rotate the log file.
    async fn rotate(&self) -> Result<(), IndexerError> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let rotated_name = format!(
            "{}.{}",
            self.path.file_name().unwrap_or_default().to_string_lossy(),
            timestamp
        );
        let rotated_path = self.path.with_file_name(rotated_name);

        tokio::fs::rename(&self.path, &rotated_path).await?;
        debug!(from = ?self.path, to = ?rotated_path, "Rotated experience log");

        Ok(())
    }

    /// Clear all entries (for testing).
    pub async fn clear(&self) -> Result<(), IndexerError> {
        if self.path.exists() {
            tokio::fs::remove_file(&self.path).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_entry() -> ExperienceEntry {
        ExperienceEntry {
            timestamp: Utc::now(),
            agent_id: "test-agent".to_string(),
            action: "test-action".to_string(),
            context: Some("test context".to_string()),
            outcome: Some("success".to_string()),
            score: Some(0.9),
        }
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let temp_dir = tempdir().unwrap();
        let log = ExperienceLog::new(temp_dir.path().join("experience.jsonl"), 1024 * 1024);

        let entry = test_entry();
        log.append(&entry).await.unwrap();

        let entries = log.read_all().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent_id, "test-agent");
    }

    #[tokio::test]
    async fn test_multiple_entries() {
        let temp_dir = tempdir().unwrap();
        let log = ExperienceLog::new(temp_dir.path().join("experience.jsonl"), 1024 * 1024);

        for i in 0..5 {
            let mut entry = test_entry();
            entry.action = format!("action-{}", i);
            log.append(&entry).await.unwrap();
        }

        let entries = log.read_all().await.unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[tokio::test]
    async fn test_count() {
        let temp_dir = tempdir().unwrap();
        let log = ExperienceLog::new(temp_dir.path().join("experience.jsonl"), 1024 * 1024);

        assert_eq!(log.count().await.unwrap(), 0);

        log.append(&test_entry()).await.unwrap();
        log.append(&test_entry()).await.unwrap();

        assert_eq!(log.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let temp_dir = tempdir().unwrap();
        let log = ExperienceLog::new(temp_dir.path().join("experience.jsonl"), 1024 * 1024);

        log.append(&test_entry()).await.unwrap();
        assert_eq!(log.count().await.unwrap(), 1);

        log.clear().await.unwrap();
        assert_eq!(log.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_rotation() {
        let temp_dir = tempdir().unwrap();
        // Very small max size to trigger rotation
        let log = ExperienceLog::new(temp_dir.path().join("experience.jsonl"), 100);

        // Write entries until rotation happens
        for _ in 0..10 {
            log.append(&test_entry()).await.unwrap();
        }

        // Check that rotated file exists
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        // Should have original file and at least one rotated file
        assert!(entries.len() >= 1);
    }

    #[tokio::test]
    async fn test_empty_log() {
        let temp_dir = tempdir().unwrap();
        let log = ExperienceLog::new(temp_dir.path().join("nonexistent.jsonl"), 1024);

        let entries = log.read_all().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_append_raw_durable() {
        let temp_dir = tempdir().unwrap();
        let log = ExperienceLog::new(temp_dir.path().join("experience.jsonl"), 1024 * 1024);

        log.append_raw_durable(r#"{"type":"memory","id":"m1"}"#)
            .await
            .unwrap();

        let content = tokio::fs::read_to_string(temp_dir.path().join("experience.jsonl"))
            .await
            .unwrap();
        assert!(content.contains(r#""id":"m1""#));
    }

    #[tokio::test]
    async fn test_read_recent_backfills_valid_entries() {
        #[derive(Debug, serde::Deserialize)]
        struct SimpleEntry {
            id: String,
        }

        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("experience.jsonl");
        let log = ExperienceLog::new(path.clone(), 1024 * 1024);

        tokio::fs::write(
            &path,
            r#"{"id":"old-1"}
{"id":"old-2"}
{"not":"matching_schema"}
"#,
        )
        .await
        .unwrap();

        let entries: Vec<SimpleEntry> = log.read_recent(2).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "old-1");
        assert_eq!(entries[1].id, "old-2");
    }

    #[test]
    fn test_entry_serialization() {
        let entry = test_entry();
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: ExperienceEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.agent_id, deserialized.agent_id);
        assert_eq!(entry.action, deserialized.action);
    }
}
