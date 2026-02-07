//! File system watcher for detecting changes.
//!
//! Uses FSEvents on macOS and inotify on Linux for efficient
//! file system event monitoring with debouncing.

use crate::IndexerError;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer, FileIdMap};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// File change type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed (old path, new path available in event)
    Renamed,
}

/// A file system change event.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path to the changed file
    pub path: PathBuf,
    /// Kind of change
    pub kind: ChangeKind,
}

/// Options for the file watcher.
#[derive(Debug, Clone)]
pub struct WatcherOptions {
    /// Debounce duration
    pub debounce_duration: Duration,
    /// Whether to watch recursively
    pub recursive: bool,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            debounce_duration: Duration::from_millis(500),
            recursive: true,
        }
    }
}

/// File system watcher with debouncing.
pub struct FileWatcher {
    options: WatcherOptions,
    tx: mpsc::Sender<FileChange>,
    rx: mpsc::Receiver<FileChange>,
    _debouncer: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
}

impl FileWatcher {
    /// Create a new file watcher.
    pub fn new(options: WatcherOptions) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        Self {
            options,
            tx,
            rx,
            _debouncer: None,
        }
    }

    /// Start watching a directory.
    pub fn watch(&mut self, path: &Path) -> Result<(), IndexerError> {
        let path = path
            .canonicalize()
            .map_err(|_| IndexerError::NotFound(path.to_path_buf()))?;

        let tx = self.tx.clone();

        // Create debounced watcher
        let mut debouncer = new_debouncer(
            self.options.debounce_duration,
            None,
            move |result: Result<Vec<DebouncedEvent>, Vec<notify::Error>>| match result {
                Ok(events) => {
                    for event in events {
                        if let Some(change) = convert_event(&event.event) {
                            if let Err(e) = tx.blocking_send(change) {
                                error!(error = %e, "Failed to send change event");
                            }
                        }
                    }
                }
                Err(errors) => {
                    for e in errors {
                        warn!(error = %e, "Watcher error");
                    }
                }
            },
        )
        .map_err(|e| IndexerError::Watcher(e.to_string()))?;

        // Start watching - use watch() directly on debouncer (new API)
        let mode = if self.options.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        debouncer
            .watch(&path, mode)
            .map_err(|e: notify::Error| IndexerError::Watcher(e.to_string()))?;

        info!(path = ?path, recursive = self.options.recursive, "Started watching");

        self._debouncer = Some(debouncer);

        Ok(())
    }

    /// Receive the next change event.
    pub async fn next(&mut self) -> Option<FileChange> {
        self.rx.recv().await
    }

    /// Try to receive a change event without blocking.
    pub fn try_next(&mut self) -> Option<FileChange> {
        self.rx.try_recv().ok()
    }

    /// Check if there are pending events.
    pub fn has_pending(&self) -> bool {
        !self.rx.is_empty()
    }
}

/// Convert a notify Event to our FileChange.
fn convert_event(event: &Event) -> Option<FileChange> {
    let path = event.paths.first()?.clone();

    // Only care about files, not directories
    if path.is_dir() {
        return None;
    }

    let kind = match &event.kind {
        EventKind::Create(_) => ChangeKind::Created,
        EventKind::Modify(_) => ChangeKind::Modified,
        EventKind::Remove(_) => ChangeKind::Deleted,
        EventKind::Any => return None,
        EventKind::Access(_) => return None, // Ignore access events
        EventKind::Other => return None,
    };

    debug!(path = ?path, kind = ?kind, "File change detected");

    Some(FileChange { path, kind })
}

/// Batches file changes for efficient processing.
pub struct ChangeBatcher {
    changes: Vec<FileChange>,
    batch_timeout: Duration,
    last_batch: std::time::Instant,
}

impl ChangeBatcher {
    /// Create a new change batcher.
    pub fn new(batch_timeout: Duration) -> Self {
        Self {
            changes: Vec::new(),
            batch_timeout,
            last_batch: std::time::Instant::now(),
        }
    }

    /// Add a change to the batch.
    pub fn add(&mut self, change: FileChange) {
        // Deduplicate: if we already have a change for this path, update it
        if let Some(existing) = self.changes.iter_mut().find(|c| c.path == change.path) {
            // Delete always wins over modify/create
            if change.kind == ChangeKind::Deleted {
                existing.kind = ChangeKind::Deleted;
            } else if existing.kind != ChangeKind::Deleted {
                existing.kind = change.kind;
            }
        } else {
            self.changes.push(change);
        }
    }

    /// Check if the batch is ready to process.
    pub fn is_ready(&self) -> bool {
        !self.changes.is_empty() && self.last_batch.elapsed() >= self.batch_timeout
    }

    /// Take the current batch and reset.
    pub fn take(&mut self) -> Vec<FileChange> {
        self.last_batch = std::time::Instant::now();
        std::mem::take(&mut self.changes)
    }

    /// Get the number of pending changes.
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_watcher_options_default() {
        let options = WatcherOptions::default();
        assert_eq!(options.debounce_duration, Duration::from_millis(500));
        assert!(options.recursive);
    }

    #[tokio::test]
    async fn test_watcher_create() {
        let temp_dir = tempdir().unwrap();
        let mut watcher = FileWatcher::new(WatcherOptions::default());

        let result = watcher.watch(temp_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_change_batcher_add() {
        let mut batcher = ChangeBatcher::new(Duration::from_millis(100));

        batcher.add(FileChange {
            path: PathBuf::from("test.rs"),
            kind: ChangeKind::Modified,
        });

        assert_eq!(batcher.len(), 1);
    }

    #[test]
    fn test_change_batcher_deduplication() {
        let mut batcher = ChangeBatcher::new(Duration::from_millis(100));

        batcher.add(FileChange {
            path: PathBuf::from("test.rs"),
            kind: ChangeKind::Modified,
        });
        batcher.add(FileChange {
            path: PathBuf::from("test.rs"),
            kind: ChangeKind::Modified,
        });

        assert_eq!(batcher.len(), 1);
    }

    #[test]
    fn test_change_batcher_delete_wins() {
        let mut batcher = ChangeBatcher::new(Duration::from_millis(100));

        batcher.add(FileChange {
            path: PathBuf::from("test.rs"),
            kind: ChangeKind::Modified,
        });
        batcher.add(FileChange {
            path: PathBuf::from("test.rs"),
            kind: ChangeKind::Deleted,
        });

        let batch = batcher.take();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].kind, ChangeKind::Deleted);
    }

    #[test]
    fn test_change_batcher_take() {
        let mut batcher = ChangeBatcher::new(Duration::from_millis(100));

        batcher.add(FileChange {
            path: PathBuf::from("a.rs"),
            kind: ChangeKind::Created,
        });
        batcher.add(FileChange {
            path: PathBuf::from("b.rs"),
            kind: ChangeKind::Modified,
        });

        let batch = batcher.take();
        assert_eq!(batch.len(), 2);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_convert_event_create() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("test.rs")],
            attrs: Default::default(),
        };

        let change = convert_event(&event);
        assert!(change.is_some());
        assert_eq!(change.unwrap().kind, ChangeKind::Created);
    }

    #[test]
    fn test_convert_event_modify() {
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("test.rs")],
            attrs: Default::default(),
        };

        let change = convert_event(&event);
        assert!(change.is_some());
        assert_eq!(change.unwrap().kind, ChangeKind::Modified);
    }

    #[test]
    fn test_convert_event_delete() {
        let event = Event {
            kind: EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![PathBuf::from("test.rs")],
            attrs: Default::default(),
        };

        let change = convert_event(&event);
        assert!(change.is_some());
        assert_eq!(change.unwrap().kind, ChangeKind::Deleted);
    }

    #[test]
    fn test_convert_event_access_ignored() {
        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("test.rs")],
            attrs: Default::default(),
        };

        let change = convert_event(&event);
        assert!(change.is_none());
    }

    #[test]
    fn test_change_kind_equality() {
        assert_eq!(ChangeKind::Created, ChangeKind::Created);
        assert_ne!(ChangeKind::Created, ChangeKind::Modified);
    }
}
