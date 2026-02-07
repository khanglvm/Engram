//! Request handler for daemon IPC.

use async_trait::async_trait;
use engram_context::{ContextManager, ContextRenderer, MemoryStore, ScopeRequest};
use engram_core::{Metrics, ProjectManager};
use engram_indexer::storage::Storage;
use engram_ipc::{ErrorCode, Request, RequestHandler, Response, ResponseData};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Handles incoming IPC requests
pub struct DaemonHandler {
    project_manager: Arc<ProjectManager>,
    memory_store: Arc<MemoryStore>,
    context_manager: Arc<ContextManager>,
    context_renderer: ContextRenderer,
    shutdown_tx: broadcast::Sender<()>,
    start_time: Instant,
    /// Metrics for request tracking
    metrics: Arc<Metrics>,
}

impl DaemonHandler {
    /// Create a new handler
    pub fn new(
        project_manager: Arc<ProjectManager>,
        storage: Arc<Storage>,
        shutdown_tx: broadcast::Sender<()>,
        start_time: Instant,
    ) -> Self {
        let context_manager = Arc::new(ContextManager::new(storage.clone()));
        let context_renderer = ContextRenderer::new();
        let memory_store = Arc::new(MemoryStore::new(storage));

        Self {
            project_manager,
            memory_store,
            context_manager,
            context_renderer,
            shutdown_tx,
            start_time,
            metrics: Arc::new(Metrics::new()),
        }
    }

    /// Get uptime in seconds
    fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

#[async_trait]
impl RequestHandler for DaemonHandler {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Ping => Response::ok_with(ResponseData::Pong {
                timestamp: chrono::Utc::now().timestamp(),
            }),

            Request::Status => {
                let projects_loaded = self.project_manager.loaded_count().await;
                let requests_total = self.metrics.requests_total.load(Ordering::Relaxed);
                let cache_hit_rate = self.metrics.cache_hit_rate();
                let avg_latency_ms = self.metrics.avg_latency().as_millis() as u64;

                Response::ok_with(ResponseData::Status {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    uptime_secs: self.uptime_secs(),
                    projects_loaded,
                    memory_usage_bytes: get_memory_usage(),
                    requests_total,
                    cache_hit_rate,
                    avg_latency_ms,
                })
            }

            Request::CheckInit { cwd } => {
                let initialized = self.project_manager.is_initialized(&cwd).await;
                Response::ok_with(ResponseData::InitStatus { initialized })
            }

            Request::InitProject { cwd, async_mode: _ } => {
                match self.project_manager.init_project(&cwd).await {
                    Ok(project) => {
                        tracing::info!(
                            project = ?project.path,
                            "Project initialized"
                        );
                        Response::ok()
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to init project");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::GetContext { cwd, prompt: _ } => {
                // Check if project is initialized
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                // Create a scope for the project
                let req = ScopeRequest::new(&cwd);
                match self.context_manager.create_scope(req).await {
                    Ok(scope) => {
                        // Get tree for rendering
                        match self.project_manager.get_tree(&cwd).await {
                            Ok(tree) => {
                                let context = self.context_renderer.render(&scope, &tree);
                                let nodes: Vec<String> = scope
                                    .focus
                                    .primary_nodes
                                    .iter()
                                    .map(|id| id.to_string())
                                    .collect();
                                Response::ok_with(ResponseData::Context { context, nodes })
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to get tree");
                                // Fall back to compact rendering without tree details
                                Response::ok_with(ResponseData::Context {
                                    context: format!("# Project Context\n\nProject: {}\n\n_(Tree unavailable: {})_", cwd.display(), e),
                                    nodes: vec![],
                                })
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to create context scope");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::PrepareContext { cwd, prompt: _ } => {
                // Fire-and-forget: prepare context for next request
                let manager = self.context_manager.clone();
                let project_manager = self.project_manager.clone();
                tokio::spawn(async move {
                    if project_manager.is_initialized(&cwd).await {
                        // Pre-create a scope to warm the cache
                        let req = ScopeRequest::new(&cwd);
                        if let Err(e) = manager.create_scope(req).await {
                            tracing::debug!(cwd = ?cwd, error = %e, "Failed to prepare context");
                        } else {
                            tracing::debug!(cwd = ?cwd, "Context prepared");
                        }
                    }
                });

                Response::ack()
            }

            Request::NotifyFileChange {
                cwd,
                path,
                change_type,
            } => {
                // Fire-and-forget: handle file change
                tracing::debug!(
                    cwd = ?cwd,
                    path = ?path,
                    change = ?change_type,
                    "File change notification"
                );

                // TODO: Phase 2 - Trigger incremental re-indexing
                Response::ack()
            }

            Request::GraftExperience { cwd, experience } => {
                // Convert IPC experience to context experience
                let mut ctx_experience =
                    engram_context::Experience::new(&experience.agent_id, &experience.decision)
                        .with_files(experience.files_touched);

                // Conditionally add rationale
                if let Some(rationale) = &experience.rationale {
                    ctx_experience = ctx_experience.with_rationale(rationale);
                }

                // Fire-and-forget: graft experience
                let manager = self.context_manager.clone();
                let cwd_clone = cwd.clone();
                tokio::spawn(async move {
                    if let Err(e) = manager.graft_experience(&cwd_clone, ctx_experience).await {
                        tracing::warn!(
                            cwd = ?cwd_clone,
                            error = %e,
                            "Failed to graft experience"
                        );
                    } else {
                        tracing::debug!(cwd = ?cwd_clone, "Experience grafted");
                    }
                });

                Response::ack()
            }

            Request::MemoryPut { cwd, entry } => {
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                if entry.kind.trim().is_empty() || entry.content.trim().is_empty() {
                    return Response::error(
                        ErrorCode::InvalidRequest,
                        "Memory entry requires non-empty kind and content",
                    );
                }

                let now = chrono::Utc::now().timestamp();
                let id = if entry.id.trim().is_empty() {
                    Uuid::new_v4().to_string()
                } else {
                    entry.id
                };

                let stored_entry = engram_ipc::MemoryEntry {
                    id: id.clone(),
                    kind: entry.kind,
                    content: entry.content,
                    tags: entry.tags,
                    created_at: if entry.created_at > 0 {
                        entry.created_at
                    } else {
                        now
                    },
                    updated_at: now,
                    session_id: entry.session_id,
                    subagent_id: entry.subagent_id,
                    deleted: entry.deleted,
                };

                match self.memory_store.put(&cwd, stored_entry).await {
                    Ok(_) => Response::ok_with(ResponseData::MemoryAck { id }),
                    Err(e) => {
                        tracing::warn!(error = %e, cwd = ?cwd, "Failed to persist memory entry");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::MemoryPatch { cwd, id, patch } => {
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                if id.trim().is_empty() {
                    return Response::error(
                        ErrorCode::InvalidRequest,
                        "Memory patch requires a non-empty id",
                    );
                }

                if patch.kind.is_none()
                    && patch.content.is_none()
                    && patch.tags.is_none()
                    && patch.session_id.is_none()
                    && patch.subagent_id.is_none()
                    && patch.deleted.is_none()
                    && patch.updated_at.is_none()
                {
                    return Response::error(
                        ErrorCode::InvalidRequest,
                        "Memory patch requires at least one field update",
                    );
                }

                if patch
                    .kind
                    .as_ref()
                    .is_some_and(|kind| kind.trim().is_empty())
                {
                    return Response::error(
                        ErrorCode::InvalidRequest,
                        "Memory patch kind cannot be empty",
                    );
                }

                if patch
                    .content
                    .as_ref()
                    .is_some_and(|content| content.trim().is_empty())
                {
                    return Response::error(
                        ErrorCode::InvalidRequest,
                        "Memory patch content cannot be empty",
                    );
                }

                match self.memory_store.patch(&cwd, &id, patch).await {
                    Ok(Some(_)) => Response::ok_with(ResponseData::MemoryAck { id }),
                    Ok(None) => Response::error(
                        ErrorCode::InvalidRequest,
                        format!("Memory entry not found: {}", id),
                    ),
                    Err(e) => {
                        tracing::warn!(error = %e, cwd = ?cwd, "Failed to patch memory entry");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::MemoryDelete { cwd, id } => {
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                if id.trim().is_empty() {
                    return Response::error(
                        ErrorCode::InvalidRequest,
                        "Memory delete requires a non-empty id",
                    );
                }

                match self.memory_store.delete(&cwd, &id, None).await {
                    Ok(Some(_)) => Response::ok_with(ResponseData::MemoryAck { id }),
                    Ok(None) => Response::error(
                        ErrorCode::InvalidRequest,
                        format!("Memory entry not found: {}", id),
                    ),
                    Err(e) => {
                        tracing::warn!(error = %e, cwd = ?cwd, "Failed to delete memory entry");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::MemoryGet { cwd, id } => {
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                match self.memory_store.get(&cwd, &id).await {
                    Ok(Some(entry)) => Response::ok_with(ResponseData::MemoryEntry { entry }),
                    Ok(None) => Response::error(
                        ErrorCode::InvalidRequest,
                        format!("Memory entry not found: {}", id),
                    ),
                    Err(e) => {
                        tracing::warn!(error = %e, cwd = ?cwd, "Failed to load memories");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::MemoryList { cwd, limit } => {
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                match self.memory_store.list(&cwd, limit).await {
                    Ok(entries) => Response::ok_with(ResponseData::MemoryEntries { entries }),
                    Err(e) => {
                        tracing::warn!(error = %e, cwd = ?cwd, "Failed to list memories");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::MemorySync { cwd } => {
                if !self.project_manager.is_initialized(&cwd).await {
                    return Response::error(
                        ErrorCode::NotInitialized,
                        "Project not initialized. Run /init-project first.",
                    );
                }

                match self.memory_store.sync(&cwd).await {
                    Ok(_) => Response::ok(),
                    Err(e) => {
                        tracing::warn!(error = %e, cwd = ?cwd, "Failed to sync memories");
                        Response::error(ErrorCode::InternalError, e.to_string())
                    }
                }
            }

            Request::Shutdown => {
                tracing::info!("Shutdown requested");
                let _ = self.shutdown_tx.send(());
                Response::ack()
            }
        }
    }
}

/// Get current memory usage in bytes
fn get_memory_usage() -> usize {
    // On macOS, we can use rusage
    #[cfg(unix)]
    {
        let mut rusage = std::mem::MaybeUninit::uninit();
        unsafe {
            if libc::getrusage(libc::RUSAGE_SELF, rusage.as_mut_ptr()) == 0 {
                let rusage = rusage.assume_init();
                // maxrss is in bytes on macOS, kilobytes on Linux
                #[cfg(target_os = "macos")]
                return rusage.ru_maxrss as usize;
                #[cfg(not(target_os = "macos"))]
                return (rusage.ru_maxrss * 1024) as usize;
            }
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::DaemonConfig;
    use engram_ipc::{MemoryEntry, MemoryPatch};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn test_handler() -> DaemonHandler {
        let temp_dir = tempdir().unwrap();
        let config = DaemonConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let manager = Arc::new(ProjectManager::new(&config));
        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let (shutdown_tx, _) = broadcast::channel(1);
        DaemonHandler::new(manager, storage, shutdown_tx, std::time::Instant::now())
    }

    fn extract_memory_ack(response: Response) -> String {
        if let Response::Ok {
            data: Some(ResponseData::MemoryAck { id }),
        } = response
        {
            id
        } else {
            panic!("Expected MemoryAck response");
        }
    }

    fn extract_memory_entry(response: Response) -> MemoryEntry {
        if let Response::Ok {
            data: Some(ResponseData::MemoryEntry { entry }),
        } = response
        {
            entry
        } else {
            panic!("Expected MemoryEntry response");
        }
    }

    fn extract_memory_entries(response: Response) -> Vec<MemoryEntry> {
        if let Response::Ok {
            data: Some(ResponseData::MemoryEntries { entries }),
        } = response
        {
            entries
        } else {
            panic!("Expected MemoryEntries response");
        }
    }

    #[tokio::test]
    async fn test_ping() {
        let handler = test_handler();
        let response = handler.handle(Request::Ping).await;

        if let Response::Ok {
            data: Some(ResponseData::Pong { .. }),
        } = response
        {
            // Success
        } else {
            panic!("Expected Pong response");
        }
    }

    #[tokio::test]
    async fn test_status() {
        let handler = test_handler();
        let response = handler.handle(Request::Status).await;

        if let Response::Ok {
            data: Some(ResponseData::Status { version, .. }),
        } = response
        {
            assert_eq!(version, env!("CARGO_PKG_VERSION"));
        } else {
            panic!("Expected Status response");
        }
    }

    #[tokio::test]
    async fn test_get_context_not_initialized() {
        let handler = test_handler();
        let response = handler
            .handle(Request::GetContext {
                cwd: PathBuf::from("/nonexistent"),
                prompt: None,
            })
            .await;

        if let Response::Error { code, .. } = response {
            assert_eq!(code, ErrorCode::NotInitialized);
        } else {
            panic!("Expected NotInitialized error");
        }
    }

    #[tokio::test]
    async fn test_memory_put_get_list_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let config = DaemonConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let manager = Arc::new(ProjectManager::new(&config));
        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let (shutdown_tx, _) = broadcast::channel(1);
        let handler = DaemonHandler::new(
            manager,
            storage.clone(),
            shutdown_tx,
            std::time::Instant::now(),
        );

        let project_dir = temp_dir.path().join("memory_project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();

        let init_response = handler
            .handle(Request::InitProject {
                cwd: project_dir.clone(),
                async_mode: false,
            })
            .await;
        assert!(matches!(init_response, Response::Ok { .. }));

        let put_response = handler
            .handle(Request::MemoryPut {
                cwd: project_dir.clone(),
                entry: MemoryEntry {
                    id: String::new(),
                    kind: "session_summary".to_string(),
                    content: "Finished phase 1 wiring".to_string(),
                    tags: vec!["phase1".to_string(), "memory".to_string()],
                    created_at: 0,
                    updated_at: 0,
                    session_id: Some("session-abc".to_string()),
                    subagent_id: None,
                    deleted: false,
                },
            })
            .await;

        let memory_id = extract_memory_ack(put_response);
        assert!(!memory_id.is_empty());

        // The ACK should only happen after the entry is durable on disk.
        let hash = storage.project_hash(&project_dir);
        let log_path = storage.project_dir(&hash).join("experience.jsonl");
        let raw = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert!(raw.contains(&memory_id));

        let get_response = handler
            .handle(Request::MemoryGet {
                cwd: project_dir.clone(),
                id: memory_id.clone(),
            })
            .await;
        let entry = extract_memory_entry(get_response);
        assert_eq!(entry.id, memory_id);
        assert_eq!(entry.kind, "session_summary");
        assert_eq!(entry.content, "Finished phase 1 wiring");
        assert_eq!(entry.tags, vec!["phase1".to_string(), "memory".to_string()]);

        let list_response = handler
            .handle(Request::MemoryList {
                cwd: project_dir,
                limit: 10,
            })
            .await;
        let entries = extract_memory_entries(list_response);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, memory_id);
    }

    #[tokio::test]
    async fn test_memory_patch_delete_sync_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let config = DaemonConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let manager = Arc::new(ProjectManager::new(&config));
        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let (shutdown_tx, _) = broadcast::channel(1);
        let handler = DaemonHandler::new(manager, storage, shutdown_tx, std::time::Instant::now());

        let project_dir = temp_dir.path().join("memory_patch_project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();

        let init_response = handler
            .handle(Request::InitProject {
                cwd: project_dir.clone(),
                async_mode: false,
            })
            .await;
        assert!(matches!(init_response, Response::Ok { .. }));

        let memory_id = extract_memory_ack(
            handler
                .handle(Request::MemoryPut {
                    cwd: project_dir.clone(),
                    entry: MemoryEntry {
                        id: String::new(),
                        kind: "decision".to_string(),
                        content: "Use in-memory index".to_string(),
                        tags: vec!["phase2".to_string()],
                        created_at: 0,
                        updated_at: 0,
                        session_id: Some("session-a".to_string()),
                        subagent_id: None,
                        deleted: false,
                    },
                })
                .await,
        );

        let patch_response = handler
            .handle(Request::MemoryPatch {
                cwd: project_dir.clone(),
                id: memory_id.clone(),
                patch: MemoryPatch {
                    content: Some("Use in-memory index with durable append".to_string()),
                    tags: Some(vec!["phase2".to_string(), "memory".to_string()]),
                    session_id: Some("session-b".to_string()),
                    ..Default::default()
                },
            })
            .await;
        assert_eq!(extract_memory_ack(patch_response), memory_id);

        let patched = extract_memory_entry(
            handler
                .handle(Request::MemoryGet {
                    cwd: project_dir.clone(),
                    id: memory_id.clone(),
                })
                .await,
        );
        assert_eq!(patched.content, "Use in-memory index with durable append");
        assert_eq!(
            patched.tags,
            vec!["phase2".to_string(), "memory".to_string()]
        );
        assert_eq!(patched.session_id, Some("session-b".to_string()));

        let delete_response = handler
            .handle(Request::MemoryDelete {
                cwd: project_dir.clone(),
                id: memory_id.clone(),
            })
            .await;
        assert_eq!(extract_memory_ack(delete_response), memory_id);

        let get_after_delete = handler
            .handle(Request::MemoryGet {
                cwd: project_dir.clone(),
                id: memory_id.clone(),
            })
            .await;
        assert!(matches!(
            get_after_delete,
            Response::Error {
                code: ErrorCode::InvalidRequest,
                ..
            }
        ));

        let sync_response = handler
            .handle(Request::MemorySync {
                cwd: project_dir.clone(),
            })
            .await;
        assert!(matches!(sync_response, Response::Ok { .. }));

        let entries = extract_memory_entries(
            handler
                .handle(Request::MemoryList {
                    cwd: project_dir,
                    limit: 10,
                })
                .await,
        );
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_memory_restart_safe_consistency() {
        let temp_dir = tempdir().unwrap();
        let config = DaemonConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let project_dir = temp_dir.path().join("memory_restart_project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();

        let handler_1 = {
            let manager = Arc::new(ProjectManager::new(&config));
            let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
            let (shutdown_tx, _) = broadcast::channel(1);
            DaemonHandler::new(manager, storage, shutdown_tx, std::time::Instant::now())
        };

        let init_response = handler_1
            .handle(Request::InitProject {
                cwd: project_dir.clone(),
                async_mode: false,
            })
            .await;
        assert!(matches!(init_response, Response::Ok { .. }));

        let first_id = extract_memory_ack(
            handler_1
                .handle(Request::MemoryPut {
                    cwd: project_dir.clone(),
                    entry: MemoryEntry {
                        id: String::new(),
                        kind: "session_summary".to_string(),
                        content: "first".to_string(),
                        tags: vec!["restart".to_string()],
                        created_at: 0,
                        updated_at: 0,
                        session_id: None,
                        subagent_id: None,
                        deleted: false,
                    },
                })
                .await,
        );
        let second_id = extract_memory_ack(
            handler_1
                .handle(Request::MemoryPut {
                    cwd: project_dir.clone(),
                    entry: MemoryEntry {
                        id: String::new(),
                        kind: "task_result".to_string(),
                        content: "second".to_string(),
                        tags: vec!["restart".to_string()],
                        created_at: 0,
                        updated_at: 0,
                        session_id: None,
                        subagent_id: None,
                        deleted: false,
                    },
                })
                .await,
        );

        let handler_2 = {
            let manager = Arc::new(ProjectManager::new(&config));
            let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
            let (shutdown_tx, _) = broadcast::channel(1);
            DaemonHandler::new(manager, storage, shutdown_tx, std::time::Instant::now())
        };

        let check_init = handler_2
            .handle(Request::CheckInit {
                cwd: project_dir.clone(),
            })
            .await;
        assert!(matches!(
            check_init,
            Response::Ok {
                data: Some(ResponseData::InitStatus { initialized: true })
            }
        ));

        let entries = extract_memory_entries(
            handler_2
                .handle(Request::MemoryList {
                    cwd: project_dir,
                    limit: 10,
                })
                .await,
        );
        let ids: HashSet<String> = entries.into_iter().map(|entry| entry.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&first_id));
        assert!(ids.contains(&second_id));
    }

    #[tokio::test]
    async fn test_memory_concurrent_writes_preserve_all_entries() {
        let temp_dir = tempdir().unwrap();
        let config = DaemonConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let manager = Arc::new(ProjectManager::new(&config));
        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let (shutdown_tx, _) = broadcast::channel(1);
        let handler = Arc::new(DaemonHandler::new(
            manager,
            storage,
            shutdown_tx,
            std::time::Instant::now(),
        ));

        let project_dir = temp_dir.path().join("memory_concurrent_project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("main.rs"), "fn main() {}").unwrap();

        let init_response = handler
            .handle(Request::InitProject {
                cwd: project_dir.clone(),
                async_mode: false,
            })
            .await;
        assert!(matches!(init_response, Response::Ok { .. }));

        let writes = 64usize;
        let mut tasks = Vec::new();
        for idx in 0..writes {
            let handler = handler.clone();
            let cwd = project_dir.clone();
            tasks.push(tokio::spawn(async move {
                extract_memory_ack(
                    handler
                        .handle(Request::MemoryPut {
                            cwd,
                            entry: MemoryEntry {
                                id: String::new(),
                                kind: "tool_observation".to_string(),
                                content: format!("entry-{idx}"),
                                tags: vec!["concurrent".to_string()],
                                created_at: 0,
                                updated_at: 0,
                                session_id: None,
                                subagent_id: Some(format!("subagent-{idx}")),
                                deleted: false,
                            },
                        })
                        .await,
                )
            }));
        }

        let mut ack_ids = HashSet::new();
        for task in tasks {
            let id = task.await.unwrap();
            ack_ids.insert(id);
        }

        assert_eq!(ack_ids.len(), writes);

        let entries = extract_memory_entries(
            handler
                .handle(Request::MemoryList {
                    cwd: project_dir,
                    limit: writes + 10,
                })
                .await,
        );
        assert_eq!(entries.len(), writes);

        let listed_ids: HashSet<String> = entries.into_iter().map(|entry| entry.id).collect();
        assert_eq!(listed_ids, ack_ids);
    }
}
