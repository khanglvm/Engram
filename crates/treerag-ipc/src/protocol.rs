//! IPC Protocol definitions for TreeRAG daemon communication.
//!
//! Uses MessagePack for efficient serialization over Unix sockets.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Request from client (hooks/CLI) to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    /// Check if project is initialized
    CheckInit { cwd: PathBuf },

    /// Initialize a new project
    InitProject {
        cwd: PathBuf,
        /// Non-blocking AI enrichment mode
        #[serde(default)]
        async_mode: bool,
    },

    /// Get context for a prompt (pre-computed cache)
    GetContext {
        cwd: PathBuf,
        prompt: Option<String>,
    },

    /// Prepare context for next prompt (async, fire-and-forget)
    PrepareContext { cwd: PathBuf, prompt: String },

    /// Notify file change (async, fire-and-forget)
    NotifyFileChange {
        cwd: PathBuf,
        path: PathBuf,
        change_type: ChangeType,
    },

    /// Graft experience from agent (async, fire-and-forget)
    GraftExperience {
        cwd: PathBuf,
        experience: Experience,
    },

    /// Store or update a memory entry
    MemoryPut { cwd: PathBuf, entry: MemoryEntry },

    /// Patch selected fields on an existing memory entry
    MemoryPatch {
        cwd: PathBuf,
        id: String,
        patch: MemoryPatch,
    },

    /// Soft-delete an existing memory entry (tombstone)
    MemoryDelete { cwd: PathBuf, id: String },

    /// Get a single memory entry by id
    MemoryGet { cwd: PathBuf, id: String },

    /// List recent memory entries
    MemoryList {
        cwd: PathBuf,
        #[serde(default = "default_memory_list_limit")]
        limit: usize,
    },

    /// Reconcile durable memory state into in-memory state
    MemorySync { cwd: PathBuf },

    /// Get daemon status
    Status,

    /// Graceful shutdown
    Shutdown,

    /// Ping for health check
    Ping,
}

/// Type of file change event
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

/// Agent experience/decision to be grafted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub agent_id: String,
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default)]
    pub files_touched: Vec<PathBuf>,
    pub timestamp: i64,
}

/// Memory entry payload (JSON/MessagePack safe)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub id: String,
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub session_id: Option<String>,
    pub subagent_id: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

/// Partial update payload for memory patch operations.
///
/// Optional fields are only applied when present.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MemoryPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// Response from daemon to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    /// Success with optional data
    Ok {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<ResponseData>,
    },

    /// Acknowledgment for fire-and-forget requests
    Ack,

    /// Error response
    Error { code: ErrorCode, message: String },
}

impl Response {
    /// Create a success response with no data
    pub fn ok() -> Self {
        Response::Ok { data: None }
    }

    /// Create a success response with data
    pub fn ok_with(data: ResponseData) -> Self {
        Response::Ok { data: Some(data) }
    }

    /// Create an acknowledgment response
    pub fn ack() -> Self {
        Response::Ack
    }

    /// Create an error response
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Response::Error {
            code,
            message: message.into(),
        }
    }
}

/// Response data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseData {
    /// Init status check result
    InitStatus { initialized: bool },

    /// Context retrieval result
    Context { context: String, nodes: Vec<String> },

    /// Daemon status
    Status {
        version: String,
        uptime_secs: u64,
        projects_loaded: usize,
        memory_usage_bytes: usize,
        /// Total requests handled
        #[serde(default)]
        requests_total: u64,
        /// Cache hit rate (0.0-1.0)
        #[serde(default)]
        cache_hit_rate: f64,
        /// Average request latency in milliseconds
        #[serde(default)]
        avg_latency_ms: u64,
    },

    /// Pong response
    Pong { timestamp: i64 },

    /// Single memory entry
    MemoryEntry { entry: MemoryEntry },

    /// Multiple memory entries
    MemoryEntries { entries: Vec<MemoryEntry> },

    /// Memory write/update acknowledgment
    MemoryAck { id: String },
}

/// Error codes for error responses
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Project has not been initialized
    NotInitialized,
    /// Request format is invalid
    InvalidRequest,
    /// Internal daemon error
    InternalError,
    /// Operation timed out
    Timeout,
    /// Daemon is shutting down
    ShuttingDown,
}

fn default_memory_list_limit() -> usize {
    50
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = Request::CheckInit {
            cwd: PathBuf::from("/test/path"),
        };

        // Test JSON serialization (for debugging)
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("check_init"));
        assert!(json.contains("/test/path"));

        // Test MessagePack round-trip
        let msgpack = rmp_serde::to_vec(&req).unwrap();
        let decoded: Request = rmp_serde::from_slice(&msgpack).unwrap();

        if let Request::CheckInit { cwd } = decoded {
            assert_eq!(cwd, PathBuf::from("/test/path"));
        } else {
            panic!("Decoded wrong variant");
        }
    }

    #[test]
    fn test_response_serialization() {
        let resp = Response::ok_with(ResponseData::Status {
            version: "0.1.0".to_string(),
            uptime_secs: 3600,
            projects_loaded: 2,
            memory_usage_bytes: 50_000_000,
            requests_total: 100,
            cache_hit_rate: 0.95,
            avg_latency_ms: 5,
        });

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ok"));
        assert!(json.contains("0.1.0"));
    }

    #[test]
    fn test_memory_put_request_roundtrip() {
        let req = Request::MemoryPut {
            cwd: PathBuf::from("/test/path"),
            entry: MemoryEntry {
                id: "mem-1".to_string(),
                kind: "decision".to_string(),
                content: "Use incremental indexing".to_string(),
                tags: vec!["indexing".to_string(), "performance".to_string()],
                created_at: 1_700_000_000,
                updated_at: 1_700_000_100,
                session_id: Some("session-1".to_string()),
                subagent_id: None,
                deleted: false,
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("memory_put"));
        assert!(json.contains("mem-1"));

        let msgpack = rmp_serde::to_vec(&req).unwrap();
        let decoded: Request = rmp_serde::from_slice(&msgpack).unwrap();

        if let Request::MemoryPut { cwd, entry } = decoded {
            assert_eq!(cwd, PathBuf::from("/test/path"));
            assert_eq!(entry.id, "mem-1");
            assert_eq!(entry.tags.len(), 2);
        } else {
            panic!("Decoded wrong variant");
        }
    }

    #[test]
    fn test_memory_patch_request_roundtrip() {
        let req = Request::MemoryPatch {
            cwd: PathBuf::from("/test/path"),
            id: "mem-1".to_string(),
            patch: MemoryPatch {
                kind: Some("task_result".to_string()),
                content: Some("Patched content".to_string()),
                tags: Some(vec!["patched".to_string(), "phase2".to_string()]),
                session_id: Some("session-2".to_string()),
                subagent_id: Some("subagent-2".to_string()),
                deleted: Some(false),
                updated_at: Some(1_700_000_200),
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("memory_patch"));
        assert!(json.contains("\"id\":\"mem-1\""));
        assert!(json.contains("\"session_id\":\"session-2\""));

        let msgpack = rmp_serde::to_vec(&req).unwrap();
        let decoded: Request = rmp_serde::from_slice(&msgpack).unwrap();

        if let Request::MemoryPatch { cwd, id, patch } = decoded {
            assert_eq!(cwd, PathBuf::from("/test/path"));
            assert_eq!(id, "mem-1");
            assert_eq!(patch.kind.as_deref(), Some("task_result"));
            assert_eq!(patch.content.as_deref(), Some("Patched content"));
            assert_eq!(
                patch.tags,
                Some(vec!["patched".to_string(), "phase2".to_string()])
            );
            assert_eq!(patch.session_id.as_deref(), Some("session-2"));
            assert_eq!(patch.subagent_id.as_deref(), Some("subagent-2"));
            assert_eq!(patch.deleted, Some(false));
            assert_eq!(patch.updated_at, Some(1_700_000_200));
        } else {
            panic!("Decoded wrong variant");
        }
    }

    #[test]
    fn test_memory_delete_and_sync_request_roundtrip() {
        let delete_req = Request::MemoryDelete {
            cwd: PathBuf::from("/test/path"),
            id: "mem-2".to_string(),
        };
        let delete_json = serde_json::to_string(&delete_req).unwrap();
        assert!(delete_json.contains("memory_delete"));
        assert!(delete_json.contains("mem-2"));

        let delete_msgpack = rmp_serde::to_vec(&delete_req).unwrap();
        let decoded_delete: Request = rmp_serde::from_slice(&delete_msgpack).unwrap();
        if let Request::MemoryDelete { cwd, id } = decoded_delete {
            assert_eq!(cwd, PathBuf::from("/test/path"));
            assert_eq!(id, "mem-2");
        } else {
            panic!("Decoded wrong delete variant");
        }

        let sync_req = Request::MemorySync {
            cwd: PathBuf::from("/test/path"),
        };
        let sync_json = serde_json::to_string(&sync_req).unwrap();
        assert!(sync_json.contains("memory_sync"));

        let sync_msgpack = rmp_serde::to_vec(&sync_req).unwrap();
        let decoded_sync: Request = rmp_serde::from_slice(&sync_msgpack).unwrap();
        if let Request::MemorySync { cwd } = decoded_sync {
            assert_eq!(cwd, PathBuf::from("/test/path"));
        } else {
            panic!("Decoded wrong sync variant");
        }
    }

    #[test]
    fn test_memory_response_roundtrip() {
        let entry = MemoryEntry {
            id: "mem-2".to_string(),
            kind: "session_summary".to_string(),
            content: "Completed migration task".to_string(),
            tags: vec!["migration".to_string()],
            created_at: 1_700_001_000,
            updated_at: 1_700_001_000,
            session_id: None,
            subagent_id: Some("subagent-1".to_string()),
            deleted: false,
        };

        let response = Response::ok_with(ResponseData::MemoryEntries {
            entries: vec![entry.clone()],
        });
        let msgpack = rmp_serde::to_vec(&response).unwrap();
        let decoded: Response = rmp_serde::from_slice(&msgpack).unwrap();

        if let Response::Ok {
            data: Some(ResponseData::MemoryEntries { entries }),
        } = decoded
        {
            assert_eq!(entries, vec![entry]);
        } else {
            panic!("Decoded wrong response variant");
        }

        let ack = Response::ok_with(ResponseData::MemoryAck {
            id: "mem-2".to_string(),
        });
        let json = serde_json::to_string(&ack).unwrap();
        assert!(json.contains("memory_ack"));
        assert!(json.contains("mem-2"));
    }
}
