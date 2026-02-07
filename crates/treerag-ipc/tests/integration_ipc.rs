//! Integration tests for TreeRAG IPC communication.
//!
//! These tests verify end-to-end communication between client and server.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;

use async_trait::async_trait;
use tempfile::tempdir;
use treerag_ipc::{
    ErrorCode, IpcClient, IpcServer, MemoryEntry, MemoryPatch, Request, RequestHandler, Response,
    ResponseData,
};

/// Test handler that simulates daemon behavior
struct IntegrationHandler;

#[async_trait]
impl RequestHandler for IntegrationHandler {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Ping => Response::ok_with(ResponseData::Pong {
                timestamp: chrono::Utc::now().timestamp(),
            }),
            Request::Status => Response::ok_with(ResponseData::Status {
                version: "0.1.0-test".to_string(),
                uptime_secs: 42,
                projects_loaded: 0,
                memory_usage_bytes: 1024,
                requests_total: 0,
                cache_hit_rate: 0.0,
                avg_latency_ms: 0,
            }),
            Request::CheckInit { cwd: _ } => {
                Response::ok_with(ResponseData::InitStatus { initialized: false })
            }
            _ => Response::ack(),
        }
    }
}

#[tokio::test]
async fn test_full_ipc_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("integration.sock");

    // Start server
    let handler = Arc::new(IntegrationHandler);
    let server = IpcServer::new(&socket_path, handler).await.unwrap();

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create client and send multiple requests
    let client = IpcClient::with_socket_path(&socket_path);

    // 1. Ping
    let response = client.request(Request::Ping).await.unwrap();
    assert!(matches!(
        response,
        Response::Ok {
            data: Some(ResponseData::Pong { .. })
        }
    ));

    // 2. Status
    let response = client.request(Request::Status).await.unwrap();
    if let Response::Ok {
        data: Some(ResponseData::Status { version, .. }),
    } = response
    {
        assert_eq!(version, "0.1.0-test");
    } else {
        panic!("Expected Status response");
    }

    // 3. CheckInit
    let response = client
        .request(Request::CheckInit {
            cwd: temp_dir.path().to_path_buf(),
        })
        .await
        .unwrap();
    if let Response::Ok {
        data: Some(ResponseData::InitStatus { initialized }),
    } = response
    {
        assert!(!initialized);
    } else {
        panic!("Expected InitStatus response");
    }
}

#[tokio::test]
async fn test_concurrent_clients() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("concurrent.sock");

    // Start server
    let handler = Arc::new(IntegrationHandler);
    let server = IpcServer::new(&socket_path, handler).await.unwrap();

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Spawn 10 concurrent clients
    let barrier = Arc::new(Barrier::new(10));
    let mut handles = Vec::new();

    for i in 0..10 {
        let path = socket_path.clone();
        let barrier = barrier.clone();

        handles.push(tokio::spawn(async move {
            // Wait for all clients to be ready
            barrier.wait().await;

            let client = IpcClient::with_socket_path(&path);
            let response = client.request(Request::Ping).await;

            (i, response.is_ok())
        }));
    }

    // Wait for all clients to complete
    let mut successes = 0;
    for handle in handles {
        let (_, success) = handle.await.unwrap();
        if success {
            successes += 1;
        }
    }

    // All clients should succeed
    assert_eq!(successes, 10, "All 10 concurrent clients should succeed");
}

#[tokio::test]
async fn test_client_reconnect() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("reconnect.sock");

    // Start server
    let handler = Arc::new(IntegrationHandler);
    let server = IpcServer::new(&socket_path, handler).await.unwrap();

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = IpcClient::with_socket_path(&socket_path);

    // First request
    let response = client.request(Request::Ping).await.unwrap();
    assert!(matches!(
        response,
        Response::Ok {
            data: Some(ResponseData::Pong { .. })
        }
    ));

    // Second request (new connection)
    let response = client.request(Request::Ping).await.unwrap();
    assert!(matches!(
        response,
        Response::Ok {
            data: Some(ResponseData::Pong { .. })
        }
    ));

    // Third request (verify still works)
    let response = client.request(Request::Status).await.unwrap();
    assert!(matches!(
        response,
        Response::Ok {
            data: Some(ResponseData::Status { .. })
        }
    ));
}

/// Test handler that stores memory entries in-process for roundtrip validation.
struct MemoryIntegrationHandler {
    memories: tokio::sync::RwLock<Vec<MemoryEntry>>,
}

#[async_trait]
impl RequestHandler for MemoryIntegrationHandler {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::MemoryPut { cwd: _, entry } => {
                self.memories.write().await.push(entry.clone());
                Response::ok_with(ResponseData::MemoryAck { id: entry.id })
            }
            Request::MemoryPatch { cwd: _, id, patch } => {
                let mut memories = self.memories.write().await;
                match memories.iter_mut().find(|entry| entry.id == id) {
                    Some(entry) => {
                        apply_patch(entry, patch);
                        Response::ok_with(ResponseData::MemoryAck { id })
                    }
                    None => Response::error(
                        ErrorCode::InvalidRequest,
                        format!("Memory not found: {}", id),
                    ),
                }
            }
            Request::MemoryDelete { cwd: _, id } => {
                let mut memories = self.memories.write().await;
                match memories.iter_mut().find(|entry| entry.id == id) {
                    Some(entry) => {
                        entry.deleted = true;
                        Response::ok_with(ResponseData::MemoryAck { id })
                    }
                    None => Response::error(
                        ErrorCode::InvalidRequest,
                        format!("Memory not found: {}", id),
                    ),
                }
            }
            Request::MemorySync { cwd: _ } => Response::ack(),
            Request::MemoryGet { cwd: _, id } => {
                let memories = self.memories.read().await;
                match memories.iter().find(|entry| entry.id == id) {
                    Some(entry) => Response::ok_with(ResponseData::MemoryEntry {
                        entry: entry.clone(),
                    }),
                    None => Response::error(
                        ErrorCode::InvalidRequest,
                        format!("Memory not found: {}", id),
                    ),
                }
            }
            Request::MemoryList { cwd: _, limit } => {
                let memories = self.memories.read().await;
                let entries = if memories.len() > limit {
                    memories[memories.len() - limit..].to_vec()
                } else {
                    memories.clone()
                };
                Response::ok_with(ResponseData::MemoryEntries { entries })
            }
            _ => Response::ack(),
        }
    }
}

fn apply_patch(entry: &mut MemoryEntry, patch: MemoryPatch) {
    if let Some(kind) = patch.kind {
        entry.kind = kind;
    }
    if let Some(content) = patch.content {
        entry.content = content;
    }
    if let Some(tags) = patch.tags {
        entry.tags = tags;
    }
    if let Some(session_id) = patch.session_id {
        entry.session_id = Some(session_id);
    }
    if let Some(subagent_id) = patch.subagent_id {
        entry.subagent_id = Some(subagent_id);
    }
    if let Some(deleted) = patch.deleted {
        entry.deleted = deleted;
    }
    if let Some(updated_at) = patch.updated_at {
        entry.updated_at = updated_at;
    }
}

#[tokio::test]
async fn test_memory_write_then_read_over_ipc() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("memory.sock");

    let handler = Arc::new(MemoryIntegrationHandler {
        memories: tokio::sync::RwLock::new(Vec::new()),
    });
    let server = IpcServer::new(&socket_path, handler).await.unwrap();

    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = IpcClient::with_socket_path(&socket_path);
    let put_entry = MemoryEntry {
        id: "mem-ipc-1".to_string(),
        kind: "decision".to_string(),
        content: "Use durable write path".to_string(),
        tags: vec!["phase1".to_string()],
        created_at: 1_700_000_000,
        updated_at: 1_700_000_000,
        session_id: None,
        subagent_id: None,
        deleted: false,
    };

    let put_response = client
        .request(Request::MemoryPut {
            cwd: temp_dir.path().to_path_buf(),
            entry: put_entry.clone(),
        })
        .await
        .unwrap();
    assert!(matches!(
        put_response,
        Response::Ok {
            data: Some(ResponseData::MemoryAck { .. })
        }
    ));

    let get_response = client
        .request(Request::MemoryGet {
            cwd: temp_dir.path().to_path_buf(),
            id: "mem-ipc-1".to_string(),
        })
        .await
        .unwrap();
    if let Response::Ok {
        data: Some(ResponseData::MemoryEntry { entry }),
    } = get_response
    {
        assert_eq!(entry, put_entry);
    } else {
        panic!("Expected MemoryEntry response");
    }

    let list_response = client
        .request(Request::MemoryList {
            cwd: temp_dir.path().to_path_buf(),
            limit: 10,
        })
        .await
        .unwrap();
    if let Response::Ok {
        data: Some(ResponseData::MemoryEntries { entries }),
    } = list_response
    {
        assert_eq!(entries, vec![put_entry]);
    } else {
        panic!("Expected MemoryEntries response");
    }
}

#[tokio::test]
async fn test_memory_patch_delete_sync_over_ipc() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("memory_phase2.sock");

    let handler = Arc::new(MemoryIntegrationHandler {
        memories: tokio::sync::RwLock::new(Vec::new()),
    });
    let server = IpcServer::new(&socket_path, handler).await.unwrap();

    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = IpcClient::with_socket_path(&socket_path);
    let entry = MemoryEntry {
        id: "mem-phase2-1".to_string(),
        kind: "decision".to_string(),
        content: "Initial content".to_string(),
        tags: vec!["phase2".to_string()],
        created_at: 1_700_000_000,
        updated_at: 1_700_000_000,
        session_id: Some("session-1".to_string()),
        subagent_id: None,
        deleted: false,
    };

    let put_response = client
        .request(Request::MemoryPut {
            cwd: temp_dir.path().to_path_buf(),
            entry: entry.clone(),
        })
        .await
        .unwrap();
    assert!(matches!(
        put_response,
        Response::Ok {
            data: Some(ResponseData::MemoryAck { .. })
        }
    ));

    let patch_response = client
        .request(Request::MemoryPatch {
            cwd: temp_dir.path().to_path_buf(),
            id: "mem-phase2-1".to_string(),
            patch: MemoryPatch {
                kind: Some("task_result".to_string()),
                content: Some("Patched content".to_string()),
                tags: Some(vec!["patched".to_string()]),
                session_id: Some("session-2".to_string()),
                subagent_id: Some("subagent-2".to_string()),
                deleted: Some(false),
                updated_at: Some(1_700_000_050),
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        patch_response,
        Response::Ok {
            data: Some(ResponseData::MemoryAck { .. })
        }
    ));

    let patched = client
        .request(Request::MemoryGet {
            cwd: temp_dir.path().to_path_buf(),
            id: "mem-phase2-1".to_string(),
        })
        .await
        .unwrap();
    if let Response::Ok {
        data: Some(ResponseData::MemoryEntry { entry }),
    } = patched
    {
        assert_eq!(entry.kind, "task_result");
        assert_eq!(entry.content, "Patched content");
        assert_eq!(entry.tags, vec!["patched".to_string()]);
        assert_eq!(entry.session_id, Some("session-2".to_string()));
        assert_eq!(entry.subagent_id, Some("subagent-2".to_string()));
        assert_eq!(entry.updated_at, 1_700_000_050);
        assert!(!entry.deleted);
    } else {
        panic!("Expected MemoryEntry response");
    }

    let delete_response = client
        .request(Request::MemoryDelete {
            cwd: temp_dir.path().to_path_buf(),
            id: "mem-phase2-1".to_string(),
        })
        .await
        .unwrap();
    assert!(matches!(
        delete_response,
        Response::Ok {
            data: Some(ResponseData::MemoryAck { .. })
        }
    ));

    let deleted = client
        .request(Request::MemoryGet {
            cwd: temp_dir.path().to_path_buf(),
            id: "mem-phase2-1".to_string(),
        })
        .await
        .unwrap();
    if let Response::Ok {
        data: Some(ResponseData::MemoryEntry { entry }),
    } = deleted
    {
        assert!(entry.deleted);
    } else {
        panic!("Expected MemoryEntry response");
    }

    let sync_response = client
        .request(Request::MemorySync {
            cwd: temp_dir.path().to_path_buf(),
        })
        .await
        .unwrap();
    assert!(matches!(sync_response, Response::Ack));
}
