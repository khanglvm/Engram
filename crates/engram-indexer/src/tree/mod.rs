//! Tree structure for representing project contents.
//!
//! Provides a hierarchical representation of files, directories,
//! and code symbols with dependency tracking.

mod builder;
mod dependency;

pub use builder::TreeBuilder;
pub use dependency::DependencyGraph;

use crate::scanner::{Framework, Language, Symbol};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Unique identifier for a tree node.
pub type NodeId = u64;

/// The complete tree representing a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tree {
    /// Tree version for format compatibility
    pub version: u32,

    /// Project root path
    pub root_path: PathBuf,

    /// All nodes in the tree, keyed by ID
    pub nodes: HashMap<NodeId, Node>,

    /// Root node ID
    pub root_id: NodeId,

    /// Dependency graph
    pub dependencies: DependencyGraph,

    /// Detected languages
    pub languages: Vec<Language>,

    /// Detected frameworks
    pub frameworks: Vec<Framework>,

    /// When this tree was created
    pub created_at: DateTime<Utc>,

    /// Last modification time
    pub updated_at: DateTime<Utc>,

    /// Total file count
    pub file_count: usize,

    /// Total symbol count
    pub symbol_count: usize,
}

impl Tree {
    /// Create a new empty tree.
    pub fn new(root_path: PathBuf) -> Self {
        let now = Utc::now();
        let root_id = 0;
        let mut nodes = HashMap::new();

        // Create root node
        let root_name = root_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string();

        nodes.insert(
            root_id,
            Node {
                id: root_id,
                name: root_name,
                path: PathBuf::new(),
                kind: NodeKind::Directory,
                parent: None,
                children: Vec::new(),
                content: None,
            },
        );

        Self {
            version: 1,
            root_path,
            nodes,
            root_id,
            dependencies: DependencyGraph::new(),
            languages: Vec::new(),
            frameworks: Vec::new(),
            created_at: now,
            updated_at: now,
            file_count: 0,
            symbol_count: 0,
        }
    }

    /// Get a node by ID.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Get a node by ID (alias for consistency with context crate).
    pub fn get_node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Get a mutable node by ID.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Get the root node.
    pub fn root(&self) -> &Node {
        self.nodes.get(&self.root_id).expect("Root node must exist")
    }

    /// Find a node by its relative path.
    pub fn find_by_path(&self, path: &PathBuf) -> Option<&Node> {
        self.nodes.values().find(|n| &n.path == path)
    }

    /// Find a node ID by its relative path.
    pub fn find_node_by_path(&self, path: &PathBuf) -> Option<NodeId> {
        self.nodes.values().find(|n| &n.path == path).map(|n| n.id)
    }

    /// Find a node ID by name (searches all nodes).
    pub fn find_node_by_name(&self, name: &str) -> Option<NodeId> {
        self.nodes.values().find(|n| n.name == name).map(|n| n.id)
    }

    /// Get all file nodes.
    pub fn files(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::File { .. }))
    }

    /// Get all symbol nodes.
    pub fn symbols(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Symbol { .. }))
    }

    /// Get children of a node.
    pub fn children(&self, id: NodeId) -> Vec<&Node> {
        self.get(id)
            .map(|n| {
                n.children
                    .iter()
                    .filter_map(|child_id| self.get(*child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update the tree's modification timestamp.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Generate a skeleton string representation of the tree.
    /// Excludes nodes in the focus set (they are shown separately).
    pub fn to_skeleton_string(&self, focus_nodes: &[NodeId]) -> String {
        let mut output = String::new();
        self.render_node_skeleton(&mut output, self.root_id, "", true, focus_nodes);
        output
    }

    /// Recursively render a node for the skeleton.
    fn render_node_skeleton(
        &self,
        output: &mut String,
        node_id: NodeId,
        prefix: &str,
        is_last: bool,
        focus_nodes: &[NodeId],
    ) {
        let Some(node) = self.get(node_id) else {
            return;
        };

        // Skip root's indentation
        if node.parent.is_some() {
            let connector = if is_last { "└── " } else { "├── " };
            let focus_marker = if focus_nodes.contains(&node_id) {
                " ← (focus)"
            } else {
                ""
            };
            output.push_str(&format!(
                "{}{}{}{}\n",
                prefix, connector, node.name, focus_marker
            ));
        } else {
            output.push_str(&format!("{}/\n", node.name));
        }

        // Render children
        let children: Vec<NodeId> = node.children.clone();
        let child_count = children.len();

        for (i, child_id) in children.iter().enumerate() {
            let is_last_child = i == child_count - 1;
            let new_prefix = if node.parent.is_some() {
                format!("{}{}   ", prefix, if is_last { " " } else { "│" })
            } else {
                String::new()
            };
            self.render_node_skeleton(output, *child_id, &new_prefix, is_last_child, focus_nodes);
        }
    }
}

/// A node in the project tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique node ID
    pub id: NodeId,

    /// Display name
    pub name: String,

    /// Relative path from project root
    pub path: PathBuf,

    /// Kind of node
    pub kind: NodeKind,

    /// Parent node ID (None for root)
    pub parent: Option<NodeId>,

    /// Child node IDs
    pub children: Vec<NodeId>,

    /// Optional content data
    pub content: Option<NodeContent>,
}

impl Node {
    /// Check if this is a directory node.
    pub fn is_directory(&self) -> bool {
        matches!(self.kind, NodeKind::Directory)
    }

    /// Check if this is a file node.
    pub fn is_file(&self) -> bool {
        matches!(self.kind, NodeKind::File { .. })
    }

    /// Check if this is a symbol node.
    pub fn is_symbol(&self) -> bool {
        matches!(self.kind, NodeKind::Symbol { .. })
    }

    /// Get the language if this is a file.
    pub fn language(&self) -> Option<Language> {
        match &self.kind {
            NodeKind::File { language, .. } => *language,
            _ => None,
        }
    }
}

/// Kind of tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeKind {
    /// Directory in the file system
    Directory,

    /// Source file
    File {
        /// Detected language
        language: Option<Language>,
        /// File size in bytes
        size: u64,
        /// Content hash
        hash: String,
        /// Line count
        line_count: usize,
    },

    /// Code symbol (function, class, etc.)
    Symbol {
        /// Symbol kind
        symbol_kind: crate::scanner::SymbolKind,
        /// Start line
        start_line: usize,
        /// End line
        end_line: usize,
    },
}

/// Additional content for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeContent {
    /// AI-generated summary
    pub summary: Option<String>,

    /// Tags/labels
    pub tags: Vec<String>,

    /// Symbols in this file (for file nodes)
    pub symbols: Vec<Symbol>,

    /// Line count (for display)
    pub line_count: usize,

    /// Content hash (for change detection)
    pub hash: String,
}

impl Default for NodeContent {
    fn default() -> Self {
        Self {
            summary: None,
            tags: Vec::new(),
            symbols: Vec::new(),
            line_count: 0,
            hash: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_new() {
        let tree = Tree::new(PathBuf::from("/test/project"));

        assert_eq!(tree.version, 1);
        assert_eq!(tree.root_path, PathBuf::from("/test/project"));
        assert_eq!(tree.file_count, 0);
        assert!(!tree.nodes.is_empty());
    }

    #[test]
    fn test_tree_root() {
        let tree = Tree::new(PathBuf::from("/test/project"));
        let root = tree.root();

        assert_eq!(root.id, 0);
        assert_eq!(root.name, "project");
        assert!(root.is_directory());
    }

    #[test]
    fn test_node_is_methods() {
        let dir_node = Node {
            id: 1,
            name: "src".to_string(),
            path: PathBuf::from("src"),
            kind: NodeKind::Directory,
            parent: Some(0),
            children: vec![],
            content: None,
        };

        let file_node = Node {
            id: 2,
            name: "main.rs".to_string(),
            path: PathBuf::from("src/main.rs"),
            kind: NodeKind::File {
                language: Some(Language::Rust),
                size: 100,
                hash: "abc".to_string(),
                line_count: 10,
            },
            parent: Some(1),
            children: vec![],
            content: None,
        };

        assert!(dir_node.is_directory());
        assert!(!dir_node.is_file());

        assert!(file_node.is_file());
        assert!(!file_node.is_directory());
        assert_eq!(file_node.language(), Some(Language::Rust));
    }

    #[test]
    fn test_tree_serialization() {
        let tree = Tree::new(PathBuf::from("/test"));

        let json = serde_json::to_string(&tree).unwrap();
        let deserialized: Tree = serde_json::from_str(&json).unwrap();

        assert_eq!(tree.version, deserialized.version);
        assert_eq!(tree.root_path, deserialized.root_path);
    }

    #[test]
    fn test_tree_touch() {
        let mut tree = Tree::new(PathBuf::from("/test"));
        let original = tree.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        tree.touch();

        assert!(tree.updated_at > original);
    }
}
