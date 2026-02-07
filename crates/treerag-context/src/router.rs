//! Hybrid retrieval router for context queries.
//!
//! Routes queries to appropriate indexes (tree-based or semantic)
//! based on query intent classification.

use crate::scope::ContextScope;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;
use treerag_indexer::tree::{NodeId, Tree};

/// Query intent classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryIntent {
    /// Structural query (e.g., "What calls X?")
    Structural,
    /// Semantic query (e.g., "How does auth work?")
    Semantic,
    /// Hybrid query (needs both indexes)
    Hybrid,
}

/// A retrieval result with score and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// Node ID in the tree
    pub node_id: NodeId,
    /// Relevance score (0.0 - 1.0)
    pub score: f32,
    /// Source of the result
    pub source: ResultSource,
    /// Snippet of matching content
    pub snippet: Option<String>,
}

/// Source of a retrieval result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResultSource {
    /// From tree/dependency index
    Tree,
    /// From vector/semantic index
    Vector,
    /// From merged results
    Merged,
}

/// Hybrid retrieval router.
pub struct HybridRouter {
    /// Tree structure
    tree: Arc<Tree>,
    /// Query classifier
    classifier: QueryClassifier,
    // Future: vector_index: Option<VectorIndex>,
}

impl HybridRouter {
    /// Create a new hybrid router.
    pub fn new(tree: Arc<Tree>) -> Self {
        Self {
            tree,
            classifier: QueryClassifier::new(),
        }
    }

    /// Query the indexes based on intent classification.
    pub fn query(&self, q: &str, scope: &ContextScope) -> Vec<RetrievalResult> {
        let intent = self.classifier.classify(q);
        debug!(query = %q, intent = ?intent, "Query classified");

        match intent {
            QueryIntent::Structural => self.query_tree(q, scope),
            QueryIntent::Semantic => {
                // Future: self.query_vector(q, scope)
                // For now, fall back to tree search
                self.query_tree(q, scope)
            }
            QueryIntent::Hybrid => {
                let tree_results = self.query_tree(q, scope);
                // Future: merge with vector results
                tree_results
            }
        }
    }

    /// Query the tree index for structural information.
    fn query_tree(&self, q: &str, _scope: &ContextScope) -> Vec<RetrievalResult> {
        let q_lower = q.to_lowercase();
        let mut results = Vec::new();

        // Check for dependency queries
        if q_lower.contains("calls") || q_lower.contains("imports") || q_lower.contains("uses") {
            // Extract the target name
            if let Some(target) = self.extract_target_name(q) {
                if let Some(node_id) = self.tree.find_node_by_name(&target) {
                    // Get importers using iterator
                    for (i, importer_id) in self.tree.dependencies.imported_by(node_id).enumerate()
                    {
                        results.push(RetrievalResult {
                            node_id: importer_id,
                            score: 1.0 - (i as f32 * 0.1).min(0.9),
                            source: ResultSource::Tree,
                            snippet: None,
                        });
                    }
                }
            }
        }

        // Check for "find" queries
        if q_lower.contains("find") || q_lower.contains("locate") || q_lower.contains("where") {
            if let Some(target) = self.extract_target_name(q) {
                if let Some(node_id) = self.tree.find_node_by_name(&target) {
                    results.push(RetrievalResult {
                        node_id,
                        score: 1.0,
                        source: ResultSource::Tree,
                        snippet: None,
                    });
                }
            }
        }

        results
    }

    /// Find nodes that import a given node.
    pub fn find_importers(&self, node_id: NodeId) -> Vec<NodeId> {
        self.tree.dependencies.imported_by(node_id).collect()
    }

    /// Find nodes that a given node imports.
    pub fn find_imports(&self, node_id: NodeId) -> Vec<NodeId> {
        self.tree.dependencies.imports(node_id).collect()
    }

    /// Extract target name from query.
    fn extract_target_name(&self, q: &str) -> Option<String> {
        // Simple extraction: find quoted strings or capitalized words
        // Future: use NLP for better extraction

        // Check for quoted strings
        if let Some(start) = q.find('"') {
            if let Some(end) = q[start + 1..].find('"') {
                return Some(q[start + 1..start + 1 + end].to_string());
            }
        }

        // Check for backtick-quoted strings
        if let Some(start) = q.find('`') {
            if let Some(end) = q[start + 1..].find('`') {
                return Some(q[start + 1..start + 1 + end].to_string());
            }
        }

        // Find capitalized words or function-like patterns
        for word in q.split_whitespace() {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if word.len() > 2
                && (word
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                    || word.contains('_')
                    || word.contains("()"))
            {
                return Some(word.trim_end_matches("()").to_string());
            }
        }

        None
    }
}

/// Query intent classifier.
pub struct QueryClassifier {
    structural_patterns: Vec<&'static str>,
    semantic_patterns: Vec<&'static str>,
}

impl QueryClassifier {
    /// Create a new query classifier.
    pub fn new() -> Self {
        Self {
            structural_patterns: vec![
                "calls",
                "imports",
                "uses",
                "depends",
                "references",
                "find",
                "locate",
                "where is",
                "show me",
                "children",
                "parent",
                "contains",
            ],
            semantic_patterns: vec![
                "how does",
                "what is",
                "explain",
                "why",
                "understand",
                "describe",
                "purpose",
                "work",
                "function",
                "behavior",
            ],
        }
    }

    /// Classify the query intent.
    pub fn classify(&self, query: &str) -> QueryIntent {
        let q_lower = query.to_lowercase();

        let structural_score: usize = self
            .structural_patterns
            .iter()
            .filter(|p| q_lower.contains(*p))
            .count();

        let semantic_score: usize = self
            .semantic_patterns
            .iter()
            .filter(|p| q_lower.contains(*p))
            .count();

        if structural_score > 0 && semantic_score > 0 {
            QueryIntent::Hybrid
        } else if structural_score > semantic_score {
            QueryIntent::Structural
        } else if semantic_score > 0 {
            QueryIntent::Semantic
        } else {
            // Default to structural
            QueryIntent::Structural
        }
    }
}

impl Default for QueryClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_structural() {
        let classifier = QueryClassifier::new();

        // "calls" is structural
        assert_eq!(
            classifier.classify("Show me what calls authenticate"),
            QueryIntent::Structural
        );
        // "find" is structural
        assert_eq!(
            classifier.classify("Find the login method"),
            QueryIntent::Structural
        );
        // "imports" is structural
        assert_eq!(
            classifier.classify("Show imports of this module"),
            QueryIntent::Structural
        );
    }

    #[test]
    fn test_classify_semantic() {
        let classifier = QueryClassifier::new();

        assert_eq!(
            classifier.classify("How does authentication work?"),
            QueryIntent::Semantic
        );
        assert_eq!(
            classifier.classify("Explain the caching strategy"),
            QueryIntent::Semantic
        );
    }

    #[test]
    fn test_classify_hybrid() {
        let classifier = QueryClassifier::new();

        assert_eq!(
            classifier.classify("How does the function that calls authenticate work?"),
            QueryIntent::Hybrid
        );
    }

    #[test]
    fn test_extract_quoted_target() {
        let tree = Tree::new(std::path::PathBuf::from("/test"));
        let router = HybridRouter::new(Arc::new(tree));

        assert_eq!(
            router.extract_target_name("What calls \"authenticate\"?"),
            Some("authenticate".to_string())
        );
        assert_eq!(
            router.extract_target_name("Find `LoginService`"),
            Some("LoginService".to_string())
        );
    }

    #[test]
    fn test_result_source() {
        let result = RetrievalResult {
            node_id: 1,
            score: 0.95,
            source: ResultSource::Tree,
            snippet: Some("test".to_string()),
        };

        assert_eq!(result.source, ResultSource::Tree);
        assert_eq!(result.score, 0.95);
    }
}
