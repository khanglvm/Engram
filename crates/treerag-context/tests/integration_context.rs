//! Integration tests for TreeRAG context management flow.

use std::sync::Arc;
use tempfile::tempdir;

use treerag_context::{ContextManager, ContextRenderer, ScopeRequest};
use treerag_indexer::storage::Storage;
use treerag_indexer::tree::Tree;

/// Test context manager creation
#[tokio::test]
async fn test_context_manager_creation() {
    let temp_dir = tempdir().unwrap();
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Storage::new(storage_dir));

    let _manager = ContextManager::new(storage);
    // Should create without error
    assert!(true);
}

/// Test scope creation with existing project
#[tokio::test]
async fn test_scope_creation() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("test_project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Storage::new(storage_dir));

    // Create hash and save tree
    let hash = storage.project_hash(&project);
    let tree = Tree::new(project.clone());
    storage.save_skeleton(&tree, &hash).await.unwrap();

    let manager = ContextManager::new(storage);
    let request = ScopeRequest::new(&project);

    let scope = manager.create_scope(request).await;
    assert!(scope.is_ok(), "Scope creation should succeed: {:?}", scope);
}

/// Test context rendering
#[tokio::test]
async fn test_context_rendering() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("render_project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Storage::new(storage_dir));

    let hash = storage.project_hash(&project);
    let tree = Tree::new(project.clone());
    storage.save_skeleton(&tree, &hash).await.unwrap();

    let manager = ContextManager::new(storage);
    let renderer = ContextRenderer::new();

    let request = ScopeRequest::new(&project);
    let scope = manager.create_scope(request).await.unwrap();
    let rendered = renderer.render(&scope, &tree);

    // Should produce some output
    assert!(!rendered.is_empty(), "Should render some context");
}

/// Test concurrent context requests
#[tokio::test]
async fn test_concurrent_context_requests() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("concurrent_project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Storage::new(storage_dir.clone()));

    let hash = storage.project_hash(&project);
    let tree = Tree::new(project.clone());
    storage.save_skeleton(&tree, &hash).await.unwrap();

    let manager = Arc::new(ContextManager::new(storage));

    let mut handles = Vec::new();
    for _i in 0..5 {
        let mgr = manager.clone();
        let proj = project.clone();
        handles.push(tokio::spawn(async move {
            let request = ScopeRequest::new(&proj);
            mgr.create_scope(request).await
        }));
    }

    // All should succeed
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}

/// Test scope with constraints
#[tokio::test]
async fn test_scope_with_constraints() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("constraints_project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Storage::new(storage_dir));

    let hash = storage.project_hash(&project);
    let tree = Tree::new(project.clone());
    storage.save_skeleton(&tree, &hash).await.unwrap();

    let manager = ContextManager::new(storage);

    let request = ScopeRequest::new(&project)
        .with_constraints(vec!["no-tests".to_string(), "only-rust".to_string()]);

    let scope = manager.create_scope(request).await;
    assert!(scope.is_ok(), "Scope with constraints should succeed");
}

/// Test context manager retrieve scope
#[tokio::test]
async fn test_retrieve_scope() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("retrieve_project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Storage::new(storage_dir));

    let hash = storage.project_hash(&project);
    let tree = Tree::new(project.clone());
    storage.save_skeleton(&tree, &hash).await.unwrap();

    let manager = ContextManager::new(storage);

    let request = ScopeRequest::new(&project);
    let scope = manager.create_scope(request).await.unwrap();
    let scope_id = scope.id.clone();

    // Should be able to retrieve it
    let retrieved = manager.get_scope(&scope_id);
    assert!(retrieved.is_some(), "Should retrieve created scope");
}

/// Test durable write -> read path for memory-style entries via context storage.
#[tokio::test]
async fn test_memory_style_entry_roundtrip() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct MemoryRecord {
        id: String,
        kind: String,
        content: String,
        created_at: i64,
        updated_at: i64,
    }

    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("memory_project");
    std::fs::create_dir_all(&project).unwrap();

    let storage_dir = temp_dir.path().join("storage");
    let storage = Storage::new(storage_dir);

    let record = MemoryRecord {
        id: "mem-ctx-1".to_string(),
        kind: "context_note".to_string(),
        content: "Keep parser behavior stable".to_string(),
        created_at: 1_700_000_000,
        updated_at: 1_700_000_000,
    };

    storage
        .append_experience_durable(&project, &record)
        .await
        .unwrap();
    let loaded: Vec<MemoryRecord> = storage.load_experiences(&project, 10).await.unwrap();

    assert_eq!(loaded, vec![record]);
}
