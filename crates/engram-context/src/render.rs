//! Context rendering to injectable strings.
//!
//! Converts context scopes into markdown strings suitable for
//! injection into AI agent prompts.

use crate::scope::ContextScope;
use engram_indexer::tree::Tree;

/// Renderer for context scopes.
pub struct ContextRenderer {
    /// Maximum content size in bytes
    max_content_size: usize,
}

impl ContextRenderer {
    /// Create a new renderer with default settings.
    pub fn new() -> Self {
        Self {
            max_content_size: 100_000, // 100KB default
        }
    }

    /// Create a renderer with custom max size.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            max_content_size: max_size,
        }
    }

    /// Render a context scope to a string.
    pub fn render(&self, scope: &ContextScope, tree: &Tree) -> String {
        let mut output = String::new();
        let mut current_size = 0;

        // Header
        output.push_str("# PROJECT CONTEXT\n\n");

        // Anchor: Rules
        if !scope.anchor.rules.is_empty() {
            output.push_str("## Rules\n");
            for rule in &scope.anchor.rules {
                output.push_str(rule);
                output.push('\n');
            }
            output.push('\n');
        }

        // Anchor: Constraints
        if !scope.anchor.constraints.is_empty() {
            output.push_str("## Constraints\n");
            for constraint in &scope.anchor.constraints {
                output.push_str(&format!("- {}\n", constraint));
            }
            output.push('\n');
        }

        // Anchor: Recent Experiences
        if !scope.anchor.experiences.is_empty() {
            output.push_str("## Recent Decisions\n");
            for exp in scope.anchor.experiences.iter().rev().take(5) {
                output.push_str(&format!("- **{}**: {}\n", exp.agent_id, exp.decision));
                if let Some(rationale) = &exp.rationale {
                    output.push_str(&format!("  - Rationale: {}\n", rationale));
                }
            }
            output.push('\n');
        }

        // Focus: Primary files with content
        if !scope.focus.primary_nodes.is_empty() {
            output.push_str("## Focus Area\n\n");

            for node_id in &scope.focus.primary_nodes {
                if let Some(node) = tree.get_node(*node_id) {
                    let path = node.path.display();
                    output.push_str(&format!("### {} (primary)\n", path));

                    if let Some(content) = &node.content {
                        let content_str = self.render_node_content(content, &mut current_size);
                        output.push_str("```\n");
                        output.push_str(&content_str);
                        output.push_str("\n```\n\n");
                    }
                }
            }
        }

        // Focus: Auto-loaded dependencies
        if !scope.focus.auto_loaded.is_empty() {
            output.push_str("### Dependencies\n\n");

            for node_id in &scope.focus.auto_loaded {
                if current_size >= self.max_content_size {
                    output.push_str("_(content truncated due to size limit)_\n");
                    break;
                }

                if let Some(node) = tree.get_node(*node_id) {
                    output.push_str(&format!("#### {}\n", node.path.display()));

                    if let Some(content) = &node.content {
                        let content_str = self.render_node_content(content, &mut current_size);
                        output.push_str("```\n");
                        output.push_str(&content_str);
                        output.push_str("\n```\n\n");
                    }
                }
            }
        }

        // Horizon: Project structure
        output.push_str("## Project Structure (overview)\n\n");
        output.push_str("```\n");
        output.push_str(&scope.horizon.skeleton);
        output.push_str("\n```\n");

        output
    }

    /// Render a compact version of the context.
    pub fn render_compact(&self, scope: &ContextScope, tree: &Tree) -> String {
        let mut output = String::new();

        // Just the skeleton and focus paths
        output.push_str("# Context\n\n");

        output.push_str("## Focus\n");
        for node_id in &scope.focus.primary_nodes {
            if let Some(node) = tree.get_node(*node_id) {
                output.push_str(&format!("- {}\n", node.path.display()));
            }
        }

        output.push_str("\n## Structure\n");
        output.push_str("```\n");
        output.push_str(&scope.horizon.skeleton);
        output.push_str("\n```\n");

        output
    }

    /// Render node content with size tracking.
    fn render_node_content(
        &self,
        content: &engram_indexer::tree::NodeContent,
        current_size: &mut usize,
    ) -> String {
        // Get a summary of the content
        let summary = format!(
            "Lines: {}, Hash: {}",
            content.line_count,
            &content.hash[..8.min(content.hash.len())]
        );

        // For now, return summary. Full content would come from file reading.
        *current_size += summary.len();
        summary
    }
}

impl Default for ContextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::{AnchorContext, Experience, FocusContext, HorizonContext};
    use std::path::PathBuf;

    fn create_test_scope() -> ContextScope {
        let mut scope = ContextScope::new(PathBuf::from("/test/project"));

        scope.anchor.rules = vec!["- Use TypeScript strict mode".to_string()];
        scope.anchor.experiences = vec![Experience::new("agent-1", "Added caching")];
        scope.focus.primary_nodes = vec![1, 2];
        scope.horizon.skeleton = "src/\n├── main.ts\n└── utils/".to_string();

        scope
    }

    #[test]
    fn test_render_includes_rules() {
        let renderer = ContextRenderer::new();
        let scope = create_test_scope();
        let tree = Tree::new(PathBuf::from("/test/project"));

        let output = renderer.render(&scope, &tree);

        assert!(output.contains("## Rules"));
        assert!(output.contains("TypeScript strict mode"));
    }

    #[test]
    fn test_render_includes_experiences() {
        let renderer = ContextRenderer::new();
        let scope = create_test_scope();
        let tree = Tree::new(PathBuf::from("/test/project"));

        let output = renderer.render(&scope, &tree);

        assert!(output.contains("## Recent Decisions"));
        assert!(output.contains("Added caching"));
    }

    #[test]
    fn test_render_includes_skeleton() {
        let renderer = ContextRenderer::new();
        let scope = create_test_scope();
        let tree = Tree::new(PathBuf::from("/test/project"));

        let output = renderer.render(&scope, &tree);

        assert!(output.contains("## Project Structure"));
        assert!(output.contains("src/"));
    }

    #[test]
    fn test_render_compact() {
        let renderer = ContextRenderer::new();
        let scope = create_test_scope();
        let tree = Tree::new(PathBuf::from("/test/project"));

        let output = renderer.render_compact(&scope, &tree);

        // Compact should not include rules
        assert!(!output.contains("## Rules"));
        // But should include structure
        assert!(output.contains("## Structure"));
    }
}
