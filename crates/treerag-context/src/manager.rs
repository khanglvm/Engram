//! Context manager for AI agent sessions.
//!
//! Manages context scopes, including creation, expansion, and experience grafting.

use crate::error::{ContextError, Result};
use crate::scope::{AnchorContext, ContextScope, Experience, FocusContext, HorizonContext};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};
use treerag_indexer::storage::Storage;
use treerag_indexer::tree::{NodeId, Tree};

/// Request to create a new context scope.
#[derive(Debug, Clone)]
pub struct ScopeRequest {
    /// Project root path
    pub project_path: PathBuf,
    /// Initial focus paths (relative to project root)
    pub focus_paths: Vec<PathBuf>,
    /// Constraints from parent agent
    pub constraints: Vec<String>,
    /// Whether to auto-load dependencies
    pub auto_load_deps: bool,
}

impl ScopeRequest {
    /// Create a new scope request.
    pub fn new(project_path: impl Into<PathBuf>) -> Self {
        Self {
            project_path: project_path.into(),
            focus_paths: vec![],
            constraints: vec![],
            auto_load_deps: true,
        }
    }

    /// Add focus paths.
    pub fn with_focus(mut self, paths: Vec<PathBuf>) -> Self {
        self.focus_paths = paths;
        self
    }

    /// Add constraints.
    pub fn with_constraints(mut self, constraints: Vec<String>) -> Self {
        self.constraints = constraints;
        self
    }
}

/// Central context manager for AI agents.
pub struct ContextManager {
    /// Storage for persistence
    storage: Arc<Storage>,
    /// Active scopes (scope_id -> scope)
    scopes: RwLock<HashMap<String, ContextScope>>,
    /// Cached trees (project_hash -> tree)
    trees: RwLock<HashMap<String, Arc<Tree>>>,
}

impl ContextManager {
    /// Create a new context manager.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            scopes: RwLock::new(HashMap::new()),
            trees: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new context scope for an agent session.
    pub async fn create_scope(&self, req: ScopeRequest) -> Result<ContextScope> {
        info!(project = ?req.project_path, "Creating context scope");

        // Verify project exists
        if !req.project_path.exists() {
            return Err(ContextError::ProjectNotFound(req.project_path));
        }

        // Load or get tree
        let tree = self.get_tree(&req.project_path).await?;

        // Build scope layers
        let mut scope = ContextScope::new(req.project_path.clone());

        // Layer 1: Anchor
        scope.anchor = self
            .build_anchor(&req.project_path, &req.constraints)
            .await?;

        // Layer 2: Focus
        scope.focus = self.build_focus(&tree, &req.focus_paths, req.auto_load_deps)?;

        // Layer 3: Horizon
        scope.horizon = self.build_horizon(&tree, &scope.focus)?;

        // Store scope
        let scope_id = scope.id.clone();
        self.scopes.write().insert(scope_id.clone(), scope.clone());

        debug!(scope_id = %scope_id, "Scope created");
        Ok(scope)
    }

    /// Expand focus to include additional nodes.
    pub fn expand_focus(&self, scope_id: &str, node_ids: Vec<NodeId>) -> Result<()> {
        let mut scopes = self.scopes.write();
        let scope = scopes
            .get_mut(scope_id)
            .ok_or_else(|| ContextError::ScopeNotFound(scope_id.to_string()))?;

        for node_id in node_ids {
            if !scope.focus.expanded.contains(&node_id) {
                scope.focus.expanded.push(node_id);
            }
        }

        debug!(scope_id = %scope_id, expanded = scope.focus.expanded.len(), "Focus expanded");
        Ok(())
    }

    /// Graft experience from a completed agent.
    pub async fn graft_experience(
        &self,
        project_path: &Path,
        experience: Experience,
    ) -> Result<()> {
        info!(
            agent = %experience.agent_id,
            decision = %experience.decision,
            "Grafting experience"
        );

        // Save to experience log
        self.storage
            .append_experience(project_path, &experience)
            .await?;

        // Update any active scopes for this project
        let mut scopes = self.scopes.write();
        for scope in scopes.values_mut() {
            if scope.project_path == project_path {
                scope.anchor.experiences.push(experience.clone());
                // Keep only recent experiences
                if scope.anchor.experiences.len() > 10 {
                    scope.anchor.experiences.remove(0);
                }
            }
        }

        Ok(())
    }

    /// Get a scope by ID.
    pub fn get_scope(&self, scope_id: &str) -> Option<ContextScope> {
        self.scopes.read().get(scope_id).cloned()
    }

    /// Remove a scope.
    pub fn remove_scope(&self, scope_id: &str) -> Option<ContextScope> {
        self.scopes.write().remove(scope_id)
    }

    /// Get or load tree for a project.
    async fn get_tree(&self, project_path: &Path) -> Result<Arc<Tree>> {
        let project_hash = self.storage.project_hash(project_path);

        // Check cache
        if let Some(tree) = self.trees.read().get(&project_hash) {
            return Ok(tree.clone());
        }

        // Load from storage
        let tree = self
            .storage
            .load_tree(project_path, false)
            .await
            .map_err(|e| ContextError::Storage(e.to_string()))?;

        let tree = Arc::new(tree);
        self.trees.write().insert(project_hash, tree.clone());

        Ok(tree)
    }

    /// Build anchor context layer.
    async fn build_anchor(
        &self,
        project_path: &Path,
        constraints: &[String],
    ) -> Result<AnchorContext> {
        // Load project rules (e.g., from .treerag/rules.md or similar)
        let rules = self.load_project_rules(project_path).await;

        // Load recent experiences
        let experiences = self
            .storage
            .load_experiences(project_path, 10)
            .await
            .unwrap_or_default();

        Ok(AnchorContext {
            rules,
            experiences,
            constraints: constraints.to_vec(),
        })
    }

    /// Build focus context layer.
    fn build_focus(
        &self,
        tree: &Tree,
        focus_paths: &[PathBuf],
        auto_load: bool,
    ) -> Result<FocusContext> {
        let mut primary_nodes = Vec::new();
        let mut auto_loaded = Vec::new();

        // Find primary nodes from paths
        for path in focus_paths {
            if let Some(node_id) = tree.find_node_by_path(path) {
                primary_nodes.push(node_id);

                // Auto-load dependencies if enabled
                if auto_load {
                    for dep_id in tree.dependencies.imports(node_id) {
                        if !primary_nodes.contains(&dep_id) && !auto_loaded.contains(&dep_id) {
                            auto_loaded.push(dep_id);
                        }
                    }
                }
            } else {
                warn!(path = ?path, "Focus path not found in tree");
            }
        }

        Ok(FocusContext {
            primary_nodes,
            auto_loaded,
            expanded: vec![],
        })
    }

    /// Build horizon context layer.
    fn build_horizon(&self, tree: &Tree, focus: &FocusContext) -> Result<HorizonContext> {
        // Generate skeleton tree (directories + file names)
        let focus_nodes = focus.all_nodes();
        let skeleton = tree.to_skeleton_string(&focus_nodes);

        Ok(HorizonContext {
            skeleton,
            hot_nodes: vec![],
        })
    }

    /// Load project rules from configuration files.
    async fn load_project_rules(&self, project_path: &Path) -> Vec<String> {
        let rules_paths = [
            project_path.join(".treerag/rules.md"),
            project_path.join(".treerag/guidelines.md"),
            project_path.join("CONTRIBUTING.md"),
        ];

        let mut rules = Vec::new();

        for path in &rules_paths {
            if path.exists() {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    // Extract key rules (first few lines or bullet points)
                    for line in content.lines().take(20) {
                        let line = line.trim();
                        if line.starts_with('-') || line.starts_with('*') {
                            rules.push(line.to_string());
                        }
                    }
                }
            }
        }

        rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use treerag_indexer::tree::Tree;

    #[tokio::test]
    async fn test_create_scope_missing_project() {
        let temp_dir = tempdir().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let manager = ContextManager::new(storage);

        let req = ScopeRequest::new("/nonexistent/path");
        let result = manager.create_scope(req).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_expand_focus_not_found() {
        let temp_dir = tempdir().unwrap();
        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let manager = ContextManager::new(storage);

        let result = manager.expand_focus("nonexistent", vec![1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_request_builder() {
        let req = ScopeRequest::new("/test/project")
            .with_focus(vec![PathBuf::from("src/main.rs")])
            .with_constraints(vec!["No unsafe code".to_string()]);

        assert_eq!(req.project_path, PathBuf::from("/test/project"));
        assert_eq!(req.focus_paths.len(), 1);
        assert_eq!(req.constraints.len(), 1);
    }

    #[tokio::test]
    async fn test_create_scope_with_mixed_experience_log_formats() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct MemoryLikeEntry {
            id: String,
            kind: String,
            content: String,
        }

        let temp_dir = tempdir().unwrap();
        let project_path = temp_dir.path().join("project");
        std::fs::create_dir_all(&project_path).unwrap();
        std::fs::write(project_path.join("main.rs"), "fn main() {}").unwrap();

        let storage = Arc::new(Storage::new(temp_dir.path().to_path_buf()));
        let hash = storage.project_hash(&project_path);
        storage
            .save_skeleton(&Tree::new(project_path.clone()), &hash)
            .await
            .unwrap();

        let legacy_experience = Experience::new("legacy-agent", "legacy decision");
        storage
            .append_experience(&project_path, &legacy_experience)
            .await
            .unwrap();

        // Simulate newer memory schema written to the same backing file.
        let memory_entry = MemoryLikeEntry {
            id: "mem-1".to_string(),
            kind: "session_summary".to_string(),
            content: "summary".to_string(),
        };
        storage
            .append_experience_durable(&project_path, &memory_entry)
            .await
            .unwrap();

        let manager = ContextManager::new(storage);
        let scope = manager
            .create_scope(ScopeRequest::new(&project_path))
            .await
            .unwrap();

        // Legacy experience loading should still work in mixed-format logs.
        assert_eq!(scope.anchor.experiences.len(), 1);
        assert_eq!(scope.anchor.experiences[0].agent_id, "legacy-agent");
    }
}
