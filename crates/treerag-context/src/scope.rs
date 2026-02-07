//! Context scope and layer definitions.
//!
//! A context scope represents the complete context available to an AI agent,
//! organized into three layers: anchor, focus, and horizon.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use treerag_indexer::tree::NodeId;

/// A complete context scope for an agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextScope {
    /// Unique scope identifier
    pub id: String,
    /// Project root path
    pub project_path: PathBuf,
    /// Layer 1: Immutable anchor context
    pub anchor: AnchorContext,
    /// Layer 2: Mutable focus area
    pub focus: FocusContext,
    /// Layer 3: Read-only horizon
    pub horizon: HorizonContext,
    /// Creation timestamp
    pub created_at: i64,
}

impl ContextScope {
    /// Create a new context scope.
    pub fn new(project_path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            project_path,
            anchor: AnchorContext::default(),
            focus: FocusContext::default(),
            horizon: HorizonContext::default(),
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Get all node IDs in the focus area.
    pub fn focus_nodes(&self) -> Vec<NodeId> {
        let mut nodes = self.focus.primary_nodes.clone();
        nodes.extend(self.focus.auto_loaded.iter().cloned());
        nodes.extend(self.focus.expanded.iter().cloned());
        nodes
    }
}

/// Layer 1: Anchor context - immutable project-level information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnchorContext {
    /// Project rules and guidelines
    pub rules: Vec<String>,
    /// Recent agent experiences/decisions
    pub experiences: Vec<Experience>,
    /// Constraints from parent agent
    pub constraints: Vec<String>,
}

/// Layer 2: Focus context - mutable working area.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusContext {
    /// Primary focus files (user requested)
    pub primary_nodes: Vec<NodeId>,
    /// Auto-loaded dependencies
    pub auto_loaded: Vec<NodeId>,
    /// User-expanded nodes
    pub expanded: Vec<NodeId>,
}

impl FocusContext {
    /// Get all nodes in the focus area.
    pub fn all_nodes(&self) -> Vec<NodeId> {
        let mut nodes = self.primary_nodes.clone();
        nodes.extend(self.auto_loaded.iter().cloned());
        nodes.extend(self.expanded.iter().cloned());
        nodes
    }
}

/// Layer 3: Horizon context - read-only project overview.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HorizonContext {
    /// ASCII tree representation of project structure
    pub skeleton: String,
    /// Frequently accessed nodes (hot paths)
    pub hot_nodes: Vec<NodeId>,
}

/// An agent experience/decision record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    /// Timestamp of the decision
    pub timestamp: i64,
    /// Agent identifier
    pub agent_id: String,
    /// Session identifier
    pub session_id: String,
    /// The decision made
    pub decision: String,
    /// Rationale for the decision
    pub rationale: Option<String>,
    /// Files touched by this decision
    pub files_touched: Vec<PathBuf>,
    /// Outcome of the decision
    pub outcome: Option<Outcome>,
}

impl Experience {
    /// Create a new experience record.
    pub fn new(agent_id: impl Into<String>, decision: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Utc::now().timestamp(),
            agent_id: agent_id.into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            decision: decision.into(),
            rationale: None,
            files_touched: vec![],
            outcome: None,
        }
    }

    /// Add rationale to the experience.
    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    /// Add files touched.
    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.files_touched = files;
        self
    }

    /// Set the outcome.
    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = Some(outcome);
        self
    }
}

/// Outcome of an agent decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Outcome {
    /// Decision was successful
    Success,
    /// Decision failed with error
    Failure { error: String },
    /// Decision was reverted
    Reverted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_scope_new() {
        let scope = ContextScope::new(PathBuf::from("/test/project"));
        assert!(!scope.id.is_empty());
        assert_eq!(scope.project_path, PathBuf::from("/test/project"));
        assert!(scope.anchor.rules.is_empty());
    }

    #[test]
    fn test_focus_all_nodes() {
        let mut focus = FocusContext::default();
        focus.primary_nodes = vec![1, 2];
        focus.auto_loaded = vec![3, 4];
        focus.expanded = vec![5];

        let all = focus.all_nodes();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&1));
        assert!(all.contains(&5));
    }

    #[test]
    fn test_experience_builder() {
        let exp = Experience::new("test-agent", "Added caching layer")
            .with_rationale("Improve performance")
            .with_outcome(Outcome::Success);

        assert_eq!(exp.agent_id, "test-agent");
        assert_eq!(exp.decision, "Added caching layer");
        assert_eq!(exp.rationale, Some("Improve performance".to_string()));
        assert_eq!(exp.outcome, Some(Outcome::Success));
    }

    #[test]
    fn test_outcome_failure() {
        let outcome = Outcome::Failure {
            error: "Test error".to_string(),
        };
        if let Outcome::Failure { error } = outcome {
            assert_eq!(error, "Test error");
        } else {
            panic!("Expected Failure variant");
        }
    }
}
