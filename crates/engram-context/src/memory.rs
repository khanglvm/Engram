//! Memory store with per-project in-memory indexing and durable replay.

use chrono::Utc;
use engram_indexer::storage::Storage;
use engram_ipc::{MemoryEntry, MemoryPatch};
use parking_lot::RwLock;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Errors produced by [`MemoryStore`].
#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    /// Durable storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),
    /// Memory entry payload is invalid.
    #[error("invalid memory entry: {0}")]
    InvalidEntry(String),
    /// Patch payload is invalid or unsupported.
    #[error("invalid memory patch: {0}")]
    InvalidPatch(String),
    /// JSON serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MemoryStoreError>;

/// Sync summary for one project index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemorySyncStats {
    /// Unique IDs in the latest-by-id index (including tombstones).
    pub total_entries: usize,
    /// Non-deleted entries currently visible to readers.
    pub live_entries: usize,
    /// Deleted entries retained as tombstones.
    pub tombstones: usize,
}

/// In-memory + durable memory storage service.
///
/// Design:
/// - per-project index keyed by `MemoryEntry.id`,
/// - latest state chosen deterministically,
/// - tombstones retained in index,
/// - writes append durably before mutating memory.
pub struct MemoryStore {
    storage: Arc<Storage>,
    projects: RwLock<HashMap<String, Arc<ProjectMemory>>>,
}

struct ProjectMemory {
    gate: Mutex<()>,
    index: RwLock<ProjectIndex>,
}

#[derive(Default)]
struct ProjectIndex {
    synced: bool,
    entries: HashMap<String, MemoryEntry>,
}

struct MemoryPatchData {
    kind: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    session_id: NullableStringPatch,
    subagent_id: NullableStringPatch,
    deleted: Option<bool>,
    updated_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NullableStringPatch {
    Missing,
    Null,
    Value(String),
}

impl Default for MemoryPatchData {
    fn default() -> Self {
        Self {
            kind: None,
            content: None,
            tags: None,
            session_id: NullableStringPatch::Missing,
            subagent_id: NullableStringPatch::Missing,
            deleted: None,
            updated_at: None,
        }
    }
}

impl MemoryPatchData {
    fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.content.is_none()
            && self.tags.is_none()
            && self.session_id == NullableStringPatch::Missing
            && self.subagent_id == NullableStringPatch::Missing
            && self.deleted.is_none()
            && self.updated_at.is_none()
    }
}

impl Default for ProjectMemory {
    fn default() -> Self {
        Self {
            gate: Mutex::new(()),
            index: RwLock::new(ProjectIndex::default()),
        }
    }
}

impl MemoryStore {
    /// Create a new memory store.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            projects: RwLock::new(HashMap::new()),
        }
    }

    /// Replay durable storage and rebuild one project's in-memory index.
    pub async fn sync(&self, project_path: &Path) -> Result<MemorySyncStats> {
        let project = self.project_memory(project_path);
        let _guard = project.gate.lock().await;

        let entries = self.rebuild_from_storage(project_path).await?;
        let stats = stats_for_entries(&entries);

        let mut index = project.index.write();
        index.entries = entries;
        index.synced = true;

        Ok(stats)
    }

    /// Insert a new memory entry version (durable append + in-memory apply).
    pub async fn put(&self, project_path: &Path, mut entry: MemoryEntry) -> Result<MemoryEntry> {
        if entry.id.trim().is_empty() {
            entry.id = Uuid::new_v4().to_string();
        }

        let now = current_timestamp();
        if entry.created_at <= 0 {
            entry.created_at = now;
        }
        if entry.updated_at <= 0 {
            entry.updated_at = now;
        }
        validate_entry(&entry)?;

        let project = self.project_memory(project_path);
        let _guard = project.gate.lock().await;
        self.ensure_synced_locked(project_path, &project).await?;

        self.storage
            .append_experience_durable(project_path, &entry)
            .await
            .map_err(|e| MemoryStoreError::Storage(e.to_string()))?;

        let mut index = project.index.write();
        apply_latest(&mut index.entries, entry.clone());

        Ok(index
            .entries
            .get(&entry.id)
            .cloned()
            .expect("entry must exist after apply"))
    }

    /// Get latest entry by ID including tombstones.
    pub async fn get_latest(&self, project_path: &Path, id: &str) -> Result<Option<MemoryEntry>> {
        let project = self.project_memory(project_path);
        self.ensure_synced(project_path, &project).await?;
        let entry = {
            let index = project.index.read();
            index.entries.get(id).cloned()
        };
        Ok(entry)
    }

    /// Get latest non-deleted entry by ID.
    pub async fn get(&self, project_path: &Path, id: &str) -> Result<Option<MemoryEntry>> {
        Ok(self
            .get_latest(project_path, id)
            .await?
            .filter(|entry| !entry.deleted))
    }

    /// List latest non-deleted entries ordered by recency, oldest to newest.
    pub async fn list(&self, project_path: &Path, limit: usize) -> Result<Vec<MemoryEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let project = self.project_memory(project_path);
        self.ensure_synced(project_path, &project).await?;

        let index = project.index.read();
        let mut entries: Vec<MemoryEntry> = index
            .entries
            .values()
            .filter(|entry| !entry.deleted)
            .cloned()
            .collect();
        entries.sort_by(compare_entries);

        if entries.len() > limit {
            entries.drain(..entries.len() - limit);
        }

        Ok(entries)
    }

    /// Patch an existing entry version using an IPC-compatible payload.
    ///
    /// The payload is normalized via serde, so any IPC patch struct that
    /// serializes with these fields is supported.
    pub async fn patch(
        &self,
        project_path: &Path,
        id: &str,
        patch: MemoryPatch,
    ) -> Result<Option<MemoryEntry>> {
        if id.trim().is_empty() {
            return Err(MemoryStoreError::InvalidEntry(
                "memory id cannot be empty".to_string(),
            ));
        }

        let patch = normalize_patch(patch)?;
        let project = self.project_memory(project_path);
        let _guard = project.gate.lock().await;
        self.ensure_synced_locked(project_path, &project).await?;

        let current = {
            let index = project.index.read();
            index.entries.get(id).cloned()
        };
        let Some(current) = current else {
            return Ok(None);
        };

        let mut updated = current.clone();
        if let Some(kind) = patch.kind {
            updated.kind = kind;
        }
        if let Some(content) = patch.content {
            updated.content = content;
        }
        if let Some(tags) = patch.tags {
            updated.tags = tags;
        }
        match patch.session_id {
            NullableStringPatch::Missing => {}
            NullableStringPatch::Null => {
                updated.session_id = None;
            }
            NullableStringPatch::Value(value) => {
                updated.session_id = Some(value);
            }
        }
        match patch.subagent_id {
            NullableStringPatch::Missing => {}
            NullableStringPatch::Null => {
                updated.subagent_id = None;
            }
            NullableStringPatch::Value(value) => {
                updated.subagent_id = Some(value);
            }
        }
        if let Some(deleted) = patch.deleted {
            updated.deleted = deleted;
        }

        let now = current_timestamp();
        let patched_updated_at = patch.updated_at.unwrap_or(now);
        updated.updated_at =
            std::cmp::max(patched_updated_at, current.updated_at.saturating_add(1));
        updated.id = id.to_string();
        validate_entry(&updated)?;

        self.storage
            .append_experience_durable(project_path, &updated)
            .await
            .map_err(|e| MemoryStoreError::Storage(e.to_string()))?;

        let mut index = project.index.write();
        apply_latest(&mut index.entries, updated.clone());
        Ok(index.entries.get(id).cloned())
    }

    /// Soft-delete an entry by appending a tombstone version.
    pub async fn delete(
        &self,
        project_path: &Path,
        id: &str,
        deleted_at: Option<i64>,
    ) -> Result<Option<MemoryEntry>> {
        if id.trim().is_empty() {
            return Err(MemoryStoreError::InvalidEntry(
                "memory id cannot be empty".to_string(),
            ));
        }

        let project = self.project_memory(project_path);
        let _guard = project.gate.lock().await;
        self.ensure_synced_locked(project_path, &project).await?;

        let current = {
            let index = project.index.read();
            index.entries.get(id).cloned()
        };
        let Some(current) = current else {
            return Ok(None);
        };

        let now = current_timestamp();
        let candidate_updated_at = deleted_at.unwrap_or(now);
        let mut tombstone = current.clone();
        tombstone.deleted = true;
        tombstone.updated_at =
            std::cmp::max(candidate_updated_at, current.updated_at.saturating_add(1));

        self.storage
            .append_experience_durable(project_path, &tombstone)
            .await
            .map_err(|e| MemoryStoreError::Storage(e.to_string()))?;

        let mut index = project.index.write();
        apply_latest(&mut index.entries, tombstone.clone());
        Ok(index.entries.get(id).cloned())
    }

    fn project_memory(&self, project_path: &Path) -> Arc<ProjectMemory> {
        let hash = self.storage.project_hash(project_path);

        if let Some(project) = self.projects.read().get(&hash).cloned() {
            return project;
        }

        let mut projects = self.projects.write();
        projects
            .entry(hash)
            .or_insert_with(|| Arc::new(ProjectMemory::default()))
            .clone()
    }

    async fn ensure_synced(&self, project_path: &Path, project: &ProjectMemory) -> Result<()> {
        if project.index.read().synced {
            return Ok(());
        }

        let _guard = project.gate.lock().await;
        self.ensure_synced_locked(project_path, project).await
    }

    async fn ensure_synced_locked(
        &self,
        project_path: &Path,
        project: &ProjectMemory,
    ) -> Result<()> {
        if project.index.read().synced {
            return Ok(());
        }

        let entries = self.rebuild_from_storage(project_path).await?;
        let mut index = project.index.write();
        index.entries = entries;
        index.synced = true;

        Ok(())
    }

    async fn rebuild_from_storage(
        &self,
        project_path: &Path,
    ) -> Result<HashMap<String, MemoryEntry>> {
        let all_entries: Vec<MemoryEntry> =
            self.storage
                .load_all_experiences(project_path)
                .await
                .map_err(|e| MemoryStoreError::Storage(e.to_string()))?;

        let mut latest_by_id = HashMap::new();
        for entry in all_entries {
            apply_latest(&mut latest_by_id, entry);
        }

        Ok(latest_by_id)
    }
}

fn current_timestamp() -> i64 {
    Utc::now().timestamp()
}

fn validate_entry(entry: &MemoryEntry) -> Result<()> {
    if entry.id.trim().is_empty() {
        return Err(MemoryStoreError::InvalidEntry(
            "memory id cannot be empty".to_string(),
        ));
    }
    if entry.kind.trim().is_empty() {
        return Err(MemoryStoreError::InvalidEntry(
            "memory kind cannot be empty".to_string(),
        ));
    }
    if entry.content.trim().is_empty() {
        return Err(MemoryStoreError::InvalidEntry(
            "memory content cannot be empty".to_string(),
        ));
    }
    if entry.created_at <= 0 {
        return Err(MemoryStoreError::InvalidEntry(
            "memory created_at must be positive".to_string(),
        ));
    }
    if entry.updated_at <= 0 {
        return Err(MemoryStoreError::InvalidEntry(
            "memory updated_at must be positive".to_string(),
        ));
    }
    Ok(())
}

fn normalize_patch(patch: MemoryPatch) -> Result<MemoryPatchData> {
    let value = serde_json::to_value(patch)?;
    let Some(object) = value.as_object() else {
        return Err(MemoryStoreError::InvalidPatch(
            "patch payload must serialize to an object".to_string(),
        ));
    };

    let mut patch = MemoryPatchData::default();
    if let Some(raw) = object.get("kind") {
        patch.kind = Some(serde_json::from_value(raw.clone())?);
    }
    if let Some(raw) = object.get("content") {
        patch.content = Some(serde_json::from_value(raw.clone())?);
    }
    if let Some(raw) = object.get("tags") {
        patch.tags = Some(serde_json::from_value(raw.clone())?);
    }
    if let Some(raw) = object.get("session_id") {
        patch.session_id = if raw.is_null() {
            NullableStringPatch::Null
        } else {
            NullableStringPatch::Value(serde_json::from_value(raw.clone())?)
        };
    }
    if let Some(raw) = object.get("subagent_id") {
        patch.subagent_id = if raw.is_null() {
            NullableStringPatch::Null
        } else {
            NullableStringPatch::Value(serde_json::from_value(raw.clone())?)
        };
    }
    if let Some(raw) = object.get("deleted") {
        patch.deleted = Some(serde_json::from_value(raw.clone())?);
    }
    if let Some(raw) = object.get("updated_at") {
        patch.updated_at = Some(serde_json::from_value(raw.clone())?);
    }

    if patch.is_empty() {
        return Err(MemoryStoreError::InvalidPatch(
            "patch payload has no supported fields".to_string(),
        ));
    }

    if matches!(patch.kind.as_deref(), Some(kind) if kind.trim().is_empty()) {
        return Err(MemoryStoreError::InvalidPatch(
            "kind cannot be empty".to_string(),
        ));
    }
    if matches!(patch.content.as_deref(), Some(content) if content.trim().is_empty()) {
        return Err(MemoryStoreError::InvalidPatch(
            "content cannot be empty".to_string(),
        ));
    }

    Ok(patch)
}

fn stats_for_entries(entries: &HashMap<String, MemoryEntry>) -> MemorySyncStats {
    let total_entries = entries.len();
    let tombstones = entries.values().filter(|entry| entry.deleted).count();
    MemorySyncStats {
        total_entries,
        live_entries: total_entries - tombstones,
        tombstones,
    }
}

fn apply_latest(latest_by_id: &mut HashMap<String, MemoryEntry>, candidate: MemoryEntry) {
    match latest_by_id.get(&candidate.id) {
        Some(current) if compare_entries(current, &candidate).is_ge() => {}
        _ => {
            latest_by_id.insert(candidate.id.clone(), candidate);
        }
    }
}

fn compare_entries(left: &MemoryEntry, right: &MemoryEntry) -> Ordering {
    left.updated_at
        .cmp(&right.updated_at)
        .then_with(|| left.created_at.cmp(&right.created_at))
        .then_with(|| left.deleted.cmp(&right.deleted))
        .then_with(|| left.id.cmp(&right.id))
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| left.content.cmp(&right.content))
        .then_with(|| left.tags.cmp(&right.tags))
        .then_with(|| left.session_id.cmp(&right.session_id))
        .then_with(|| left.subagent_id.cmp(&right.subagent_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_ipc::MemoryPatch;
    use tempfile::tempdir;
    use tokio::task::JoinSet;

    fn test_entry(id: &str, content: &str, updated_at: i64) -> MemoryEntry {
        MemoryEntry {
            id: id.to_string(),
            kind: "context_note".to_string(),
            content: content.to_string(),
            tags: vec!["phase2".to_string()],
            created_at: 1_700_000_000,
            updated_at,
            session_id: Some("session-1".to_string()),
            subagent_id: None,
            deleted: false,
        }
    }

    #[tokio::test]
    async fn test_replay_rebuild_correctness() {
        let temp_dir = tempdir().unwrap();
        let project = temp_dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        let storage = Arc::new(Storage::new(temp_dir.path().join("storage")));

        let base = test_entry("mem-1", "initial", 10);
        let updated = MemoryEntry {
            content: "patched".to_string(),
            updated_at: 20,
            ..base.clone()
        };
        let second = test_entry("mem-2", "to-be-deleted", 12);
        let tombstone = MemoryEntry {
            deleted: true,
            updated_at: 30,
            ..second.clone()
        };

        storage
            .append_experience_durable(&project, &base)
            .await
            .unwrap();
        storage
            .append_experience_durable(&project, &updated)
            .await
            .unwrap();
        storage
            .append_experience_durable(&project, &second)
            .await
            .unwrap();
        storage
            .append_experience_durable(&project, &tombstone)
            .await
            .unwrap();

        let store = MemoryStore::new(storage.clone());
        let stats = store.sync(&project).await.unwrap();
        assert_eq!(
            stats,
            MemorySyncStats {
                total_entries: 2,
                live_entries: 1,
                tombstones: 1,
            }
        );

        let mem1 = store.get(&project, "mem-1").await.unwrap().unwrap();
        assert_eq!(mem1.content, "patched");
        assert_eq!(mem1.updated_at, 20);
        assert!(store.get(&project, "mem-2").await.unwrap().is_none());

        let listed = store.list(&project, 10).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "mem-1");

        // Rebuild on a fresh process view should match durable state.
        let restarted = MemoryStore::new(storage);
        let replayed = restarted.get(&project, "mem-1").await.unwrap().unwrap();
        assert_eq!(replayed.content, "patched");
        assert!(restarted.get(&project, "mem-2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_patch_delete_tombstone_behavior() {
        let temp_dir = tempdir().unwrap();
        let project = temp_dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        let storage = Arc::new(Storage::new(temp_dir.path().join("storage")));
        let store = MemoryStore::new(storage.clone());

        store
            .put(&project, test_entry("mem-1", "original", 1))
            .await
            .unwrap();

        let patched = store
            .patch(
                &project,
                "mem-1",
                MemoryPatch {
                    content: Some("patched".to_string()),
                    tags: Some(vec!["patch-applied".to_string()]),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(patched.content, "patched");
        assert_eq!(patched.tags, vec!["patch-applied".to_string()]);
        assert_eq!(patched.session_id, Some("session-1".to_string()));
        assert!(!patched.deleted);

        let tombstone = store
            .delete(&project, "mem-1", None)
            .await
            .unwrap()
            .unwrap();
        assert!(tombstone.deleted);
        assert!(store.get(&project, "mem-1").await.unwrap().is_none());
        assert_eq!(store.list(&project, 10).await.unwrap().len(), 0);

        // Patching a tombstoned entry keeps it tombstoned unless explicitly revived.
        let still_deleted = store
            .patch(
                &project,
                "mem-1",
                MemoryPatch {
                    content: Some("hidden-update".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(still_deleted.deleted);
        assert!(store.get(&project, "mem-1").await.unwrap().is_none());

        let restarted = MemoryStore::new(storage);
        let replayed_tombstone = restarted
            .get_latest(&project, "mem-1")
            .await
            .unwrap()
            .unwrap();
        assert!(replayed_tombstone.deleted);
        assert_eq!(replayed_tombstone.content, "hidden-update");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_writes_no_data_loss_and_deterministic_latest() {
        const UNIQUE_WRITES: usize = 64;
        const SHARED_WRITES: usize = 40;

        let temp_dir = tempdir().unwrap();
        let project = temp_dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        let storage = Arc::new(Storage::new(temp_dir.path().join("storage")));
        let store = Arc::new(MemoryStore::new(storage.clone()));

        let mut workers = JoinSet::new();
        for i in 0..UNIQUE_WRITES {
            let store = store.clone();
            let project = project.clone();
            workers.spawn(async move {
                let entry = test_entry(
                    &format!("unique-{i:03}"),
                    &format!("unique-content-{i:03}"),
                    100 + i as i64,
                );
                store.put(&project, entry).await
            });
        }

        for i in 0..SHARED_WRITES {
            let store = store.clone();
            let project = project.clone();
            workers.spawn(async move {
                let entry = test_entry("shared", &format!("shared-v{i:03}"), 10_000 + i as i64);
                store.put(&project, entry).await
            });
        }

        while let Some(result) = workers.join_next().await {
            result.unwrap().unwrap();
        }

        let latest_shared = store.get(&project, "shared").await.unwrap().unwrap();
        assert_eq!(
            latest_shared.updated_at,
            10_000 + (SHARED_WRITES - 1) as i64
        );
        assert_eq!(
            latest_shared.content,
            format!("shared-v{:03}", SHARED_WRITES - 1)
        );

        // Tie-breaker should still be deterministic when timestamps are equal.
        let mut tie_workers = JoinSet::new();
        for content in ["alpha", "omega", "beta"] {
            let store = store.clone();
            let project = project.clone();
            tie_workers.spawn(async move {
                let mut entry = test_entry("tie-break", content, 50_000);
                entry.created_at = 1_700_000_000;
                store.put(&project, entry).await
            });
        }

        while let Some(result) = tie_workers.join_next().await {
            result.unwrap().unwrap();
        }

        let tie_latest = store.get(&project, "tie-break").await.unwrap().unwrap();
        assert_eq!(tie_latest.content, "omega");

        let listed = store.list(&project, 1000).await.unwrap();
        assert_eq!(listed.len(), UNIQUE_WRITES + 2);

        let persisted: Vec<MemoryEntry> = storage.load_all_experiences(&project).await.unwrap();
        assert_eq!(persisted.len(), UNIQUE_WRITES + SHARED_WRITES + 3);
    }
}
