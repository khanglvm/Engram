//! Integration checks for Claude hook payload actions vs IPC protocol actions.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use engram_ipc::{ChangeType, Experience, MemoryEntry, MemoryPatch, Request};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("failed to resolve repository root")
}

fn request_action(request: Request) -> String {
    let value = serde_json::to_value(request).expect("request should serialize");
    value
        .get("action")
        .and_then(serde_json::Value::as_str)
        .expect("serialized request should contain action")
        .to_string()
}

fn supported_actions() -> HashSet<String> {
    let cwd = PathBuf::from("/tmp/project");

    vec![
        Request::CheckInit { cwd: cwd.clone() },
        Request::InitProject {
            cwd: cwd.clone(),
            async_mode: false,
        },
        Request::GetContext {
            cwd: cwd.clone(),
            prompt: None,
        },
        Request::PrepareContext {
            cwd: cwd.clone(),
            prompt: String::new(),
        },
        Request::NotifyFileChange {
            cwd: cwd.clone(),
            path: PathBuf::from("src/lib.rs"),
            change_type: ChangeType::Modified,
        },
        Request::GraftExperience {
            cwd,
            experience: Experience {
                agent_id: "agent".to_string(),
                decision: "done".to_string(),
                rationale: None,
                files_touched: vec![],
                timestamp: 0,
            },
        },
        Request::MemoryPut {
            cwd: PathBuf::from("/tmp/project"),
            entry: MemoryEntry {
                id: "mem-1".to_string(),
                kind: "session_summary".to_string(),
                content: "summary".to_string(),
                tags: vec!["hook".to_string()],
                created_at: 0,
                updated_at: 0,
                session_id: Some("session-1".to_string()),
                subagent_id: None,
                deleted: false,
            },
        },
        Request::MemoryGet {
            cwd: PathBuf::from("/tmp/project"),
            id: "mem-1".to_string(),
        },
        Request::MemoryPatch {
            cwd: PathBuf::from("/tmp/project"),
            id: "mem-1".to_string(),
            patch: MemoryPatch {
                kind: Some("task_result".to_string()),
                content: None,
                tags: Some(vec!["updated".to_string()]),
                session_id: Some("session-2".to_string()),
                subagent_id: Some("subagent-1".to_string()),
                deleted: Some(false),
                updated_at: Some(1),
            },
        },
        Request::MemoryDelete {
            cwd: PathBuf::from("/tmp/project"),
            id: "mem-1".to_string(),
        },
        Request::MemoryList {
            cwd: PathBuf::from("/tmp/project"),
            limit: 10,
        },
        Request::MemorySync {
            cwd: PathBuf::from("/tmp/project"),
        },
        Request::Status,
        Request::Shutdown,
        Request::Ping,
    ]
    .into_iter()
    .map(request_action)
    .collect()
}

fn extract_actions(script: &str) -> Vec<String> {
    let mut actions = Vec::new();
    let mut remaining = script;
    let marker = "\"action\":\"";

    while let Some(idx) = remaining.find(marker) {
        let after_marker = &remaining[idx + marker.len()..];
        if let Some(end) = after_marker.find('"') {
            actions.push(after_marker[..end].to_string());
            remaining = &after_marker[end + 1..];
        } else {
            break;
        }
    }

    actions
}

fn read_hook_actions(path: &Path) -> Vec<String> {
    let script = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("failed to read hook script {}: {e}", path.display());
    });
    extract_actions(&script)
}

#[test]
fn all_hook_actions_are_supported_by_protocol() {
    let supported = supported_actions();
    let hooks_dir = repo_root().join("claude-integration/hooks");
    let entries = fs::read_dir(&hooks_dir).expect("failed to read hooks directory");

    for entry in entries {
        let path = entry.expect("failed to read hooks entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sh") {
            continue;
        }

        for action in read_hook_actions(&path) {
            assert!(
                supported.contains(&action),
                "unknown IPC action `{action}` emitted by hook `{}`",
                path.display(),
            );
        }
    }
}

#[test]
fn session_end_hook_uses_supported_memory_write_action() {
    let supported = supported_actions();
    let hook = repo_root().join("claude-integration/hooks/session_end.sh");
    let actions = read_hook_actions(&hook);

    assert_eq!(
        actions,
        vec!["memory_put".to_string()],
        "session_end hook must emit exactly one supported memory write action",
    );
    assert!(
        supported.contains("memory_put"),
        "protocol must support memory_put",
    );
}
