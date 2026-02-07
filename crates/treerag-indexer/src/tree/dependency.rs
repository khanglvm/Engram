//! Dependency graph for import/export relationships.

use super::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Tracks dependencies between files in the project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// Forward edges: file -> files it imports
    imports: HashMap<NodeId, HashSet<NodeId>>,

    /// Reverse edges: file -> files that import it
    imported_by: HashMap<NodeId, HashSet<NodeId>>,
}

impl DependencyGraph {
    /// Create an empty dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a dependency edge: `from` imports `to`.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        self.imports.entry(from).or_default().insert(to);
        self.imported_by.entry(to).or_default().insert(from);
    }

    /// Remove a dependency edge.
    pub fn remove_edge(&mut self, from: NodeId, to: NodeId) {
        if let Some(set) = self.imports.get_mut(&from) {
            set.remove(&to);
        }
        if let Some(set) = self.imported_by.get_mut(&to) {
            set.remove(&from);
        }
    }

    /// Get all files that a given file imports.
    pub fn imports(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.imports
            .get(&node)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    /// Get all files that import a given file.
    pub fn imported_by(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.imported_by
            .get(&node)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    /// Get the number of files this node imports.
    pub fn import_count(&self, node: NodeId) -> usize {
        self.imports.get(&node).map(|s| s.len()).unwrap_or(0)
    }

    /// Get the number of files that import this node.
    pub fn imported_by_count(&self, node: NodeId) -> usize {
        self.imported_by.get(&node).map(|s| s.len()).unwrap_or(0)
    }

    /// Check if there is a dependency cycle starting from a node.
    pub fn has_cycle(&self, start: NodeId) -> bool {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        self.detect_cycle_dfs(start, &mut visited, &mut stack)
    }

    fn detect_cycle_dfs(
        &self,
        node: NodeId,
        visited: &mut HashSet<NodeId>,
        stack: &mut HashSet<NodeId>,
    ) -> bool {
        if stack.contains(&node) {
            return true; // Back edge found = cycle
        }
        if visited.contains(&node) {
            return false; // Already fully explored
        }

        visited.insert(node);
        stack.insert(node);

        for dep in self.imports(node) {
            if self.detect_cycle_dfs(dep, visited, stack) {
                return true;
            }
        }

        stack.remove(&node);
        false
    }

    /// Find all nodes involved in cycles.
    pub fn find_cycles(&self) -> Vec<Vec<NodeId>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();

        for &node in self.imports.keys() {
            if !visited.contains(&node) {
                let mut path = Vec::new();
                self.find_cycles_dfs(node, &mut visited, &mut path, &mut cycles);
            }
        }

        cycles
    }

    fn find_cycles_dfs(
        &self,
        node: NodeId,
        visited: &mut HashSet<NodeId>,
        path: &mut Vec<NodeId>,
        cycles: &mut Vec<Vec<NodeId>>,
    ) {
        if let Some(pos) = path.iter().position(|&n| n == node) {
            // Found a cycle
            cycles.push(path[pos..].to_vec());
            return;
        }

        if visited.contains(&node) {
            return;
        }

        path.push(node);

        for dep in self.imports(node) {
            self.find_cycles_dfs(dep, visited, path, cycles);
        }

        path.pop();
        visited.insert(node);
    }

    /// Remove all edges involving a node (when file is deleted).
    pub fn remove_node(&mut self, node: NodeId) {
        // Remove forward edges from this node
        if let Some(targets) = self.imports.remove(&node) {
            for target in targets {
                if let Some(set) = self.imported_by.get_mut(&target) {
                    set.remove(&node);
                }
            }
        }

        // Remove reverse edges to this node
        if let Some(sources) = self.imported_by.remove(&node) {
            for source in sources {
                if let Some(set) = self.imports.get_mut(&source) {
                    set.remove(&node);
                }
            }
        }
    }

    /// Clear all edges from a node (for re-indexing).
    pub fn clear_node(&mut self, node: NodeId) {
        // Keep the node but remove its outgoing edges
        if let Some(targets) = self.imports.remove(&node) {
            for target in targets {
                if let Some(set) = self.imported_by.get_mut(&target) {
                    set.remove(&node);
                }
            }
        }
    }

    /// Get total number of edges.
    pub fn edge_count(&self) -> usize {
        self.imports.values().map(|s| s.len()).sum()
    }

    /// Get total number of nodes with edges.
    pub fn node_count(&self) -> usize {
        let mut nodes: HashSet<NodeId> = self.imports.keys().copied().collect();
        nodes.extend(self.imported_by.keys().copied());
        nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_edge() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);

        assert!(graph.imports(1).any(|n| n == 2));
        assert!(graph.imported_by(2).any(|n| n == 1));
    }

    #[test]
    fn test_remove_edge() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.remove_edge(1, 2);

        assert_eq!(graph.imports(1).count(), 0);
        assert_eq!(graph.imported_by(2).count(), 0);
    }

    #[test]
    fn test_import_counts() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(1, 3);
        graph.add_edge(2, 3);

        assert_eq!(graph.import_count(1), 2);
        assert_eq!(graph.import_count(2), 1);
        assert_eq!(graph.imported_by_count(3), 2);
    }

    #[test]
    fn test_no_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);

        assert!(!graph.has_cycle(1));
    }

    #[test]
    fn test_has_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.add_edge(3, 1); // Creates cycle

        assert!(graph.has_cycle(1));
    }

    #[test]
    fn test_self_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 1);

        assert!(graph.has_cycle(1));
    }

    #[test]
    fn test_remove_node() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.add_edge(3, 2);

        graph.remove_node(2);

        assert_eq!(graph.imports(1).count(), 0);
        assert_eq!(graph.imports(3).count(), 0);
        assert_eq!(graph.imported_by(2).count(), 0);
    }

    #[test]
    fn test_clear_node() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(1, 3);
        graph.add_edge(4, 1);

        graph.clear_node(1);

        // Outgoing edges cleared
        assert_eq!(graph.imports(1).count(), 0);
        // Incoming edges preserved
        assert!(graph.imports(4).any(|n| n == 1));
    }

    #[test]
    fn test_edge_count() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(1, 3);
        graph.add_edge(2, 3);

        assert_eq!(graph.edge_count(), 3);
    }

    #[test]
    fn test_serialization() {
        let mut graph = DependencyGraph::new();
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);

        let json = serde_json::to_string(&graph).unwrap();
        let deserialized: DependencyGraph = serde_json::from_str(&json).unwrap();

        assert_eq!(graph.edge_count(), deserialized.edge_count());
    }
}
