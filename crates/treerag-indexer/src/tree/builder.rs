//! Tree builder from scan results.

use super::{Node, NodeContent, NodeId, NodeKind, Tree};
use crate::scanner::ScanResult;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Builds a tree from scan results.
pub struct TreeBuilder {
    next_id: NodeId,
}

impl TreeBuilder {
    /// Create a new tree builder.
    pub fn new() -> Self {
        Self { next_id: 1 } // 0 is reserved for root
    }

    /// Build a tree from scan results.
    pub fn build(&mut self, scan: &ScanResult) -> Tree {
        let mut tree = Tree::new(scan.root.clone());
        tree.languages = scan.languages.clone();
        tree.frameworks = scan.frameworks.clone();

        // Track directory nodes by path for efficient lookup
        let mut dir_map: HashMap<PathBuf, NodeId> = HashMap::new();
        dir_map.insert(PathBuf::new(), tree.root_id);

        let mut file_count = 0;
        let mut symbol_count = 0;

        for file in &scan.files {
            // Ensure parent directories exist
            let parent_id = self.ensure_directories(&file.path, &mut tree, &mut dir_map);

            // Create file node
            let file_id = self.next_id();
            let file_node = Node {
                id: file_id,
                name: file
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                path: file.path.clone(),
                kind: NodeKind::File {
                    language: file.language,
                    size: file.size,
                    hash: file.hash.clone(),
                    line_count: file.line_count,
                },
                parent: Some(parent_id),
                children: Vec::new(),
                content: Some(NodeContent {
                    summary: None,
                    tags: Vec::new(),
                    symbols: file.symbols.clone(),
                    line_count: file.line_count,
                    hash: file.hash.clone(),
                }),
            };

            // Add file to tree and parent's children
            tree.nodes.insert(file_id, file_node);
            if let Some(parent) = tree.nodes.get_mut(&parent_id) {
                parent.children.push(file_id);
            }

            file_count += 1;

            // Create symbol nodes as children of the file
            for symbol in &file.symbols {
                let symbol_id = self.next_id();
                let symbol_node = Node {
                    id: symbol_id,
                    name: symbol.name.clone(),
                    path: file.path.join(&symbol.name),
                    kind: NodeKind::Symbol {
                        symbol_kind: symbol.kind,
                        start_line: symbol.start_line,
                        end_line: symbol.end_line,
                    },
                    parent: Some(file_id),
                    children: Vec::new(),
                    content: symbol.doc.as_ref().map(|doc| NodeContent {
                        summary: Some(doc.clone()),
                        tags: Vec::new(),
                        symbols: Vec::new(),
                        line_count: 0,
                        hash: String::new(),
                    }),
                };

                tree.nodes.insert(symbol_id, symbol_node);
                if let Some(file) = tree.nodes.get_mut(&file_id) {
                    file.children.push(symbol_id);
                }

                symbol_count += 1;
            }
        }

        tree.file_count = file_count;
        tree.symbol_count = symbol_count;

        debug!(
            files = file_count,
            symbols = symbol_count,
            nodes = tree.nodes.len(),
            "Tree built"
        );

        tree
    }

    /// Ensure all parent directories exist for a path.
    fn ensure_directories(
        &mut self,
        path: &Path,
        tree: &mut Tree,
        dir_map: &mut HashMap<PathBuf, NodeId>,
    ) -> NodeId {
        let parent_path = path.parent().unwrap_or(Path::new(""));

        // If parent already exists, return its ID
        if let Some(&id) = dir_map.get(parent_path) {
            return id;
        }

        // Recursively create parent directories
        let mut current_path = PathBuf::new();
        let mut current_parent = tree.root_id;

        for component in parent_path.components() {
            current_path.push(component);

            if let Some(&id) = dir_map.get(&current_path) {
                current_parent = id;
            } else {
                // Create new directory node
                let dir_id = self.next_id();
                let dir_name = component.as_os_str().to_str().unwrap_or("").to_string();

                let dir_node = Node {
                    id: dir_id,
                    name: dir_name,
                    path: current_path.clone(),
                    kind: NodeKind::Directory,
                    parent: Some(current_parent),
                    children: Vec::new(),
                    content: None,
                };

                tree.nodes.insert(dir_id, dir_node);

                // Add to parent's children
                if let Some(parent) = tree.nodes.get_mut(&current_parent) {
                    parent.children.push(dir_id);
                }

                dir_map.insert(current_path.clone(), dir_id);
                current_parent = dir_id;
            }
        }

        current_parent
    }

    fn next_id(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Language, ScannedFile, Symbol, SymbolKind};

    fn mock_scan_result() -> ScanResult {
        ScanResult {
            root: PathBuf::from("/project"),
            files: vec![
                ScannedFile {
                    path: PathBuf::from("src/main.rs"),
                    language: Some(Language::Rust),
                    size: 100,
                    hash: "abc123".to_string(),
                    line_count: 10,
                    symbols: vec![Symbol {
                        name: "main".to_string(),
                        kind: SymbolKind::Function,
                        start_line: 1,
                        end_line: 5,
                        parent: None,
                        doc: Some("Entry point".to_string()),
                    }],
                },
                ScannedFile {
                    path: PathBuf::from("src/lib.rs"),
                    language: Some(Language::Rust),
                    size: 200,
                    hash: "def456".to_string(),
                    line_count: 20,
                    symbols: vec![],
                },
            ],
            languages: vec![Language::Rust],
            frameworks: vec![],
            duration_ms: 100,
            skipped_count: 0,
        }
    }

    #[test]
    fn test_build_tree() {
        let scan = mock_scan_result();
        let mut builder = TreeBuilder::new();
        let tree = builder.build(&scan);

        assert_eq!(tree.file_count, 2);
        assert_eq!(tree.symbol_count, 1);
        assert!(tree.nodes.len() > 3); // root + src dir + 2 files + 1 symbol
    }

    #[test]
    fn test_tree_has_directories() {
        let scan = mock_scan_result();
        let mut builder = TreeBuilder::new();
        let tree = builder.build(&scan);

        // Should have src directory
        let src = tree
            .nodes
            .values()
            .find(|n| n.name == "src" && n.is_directory());
        assert!(src.is_some());
    }

    #[test]
    fn test_tree_parent_child_relationships() {
        let scan = mock_scan_result();
        let mut builder = TreeBuilder::new();
        let tree = builder.build(&scan);

        // Find main.rs
        let main_rs = tree.nodes.values().find(|n| n.name == "main.rs").unwrap();

        // Should have a parent (src directory)
        assert!(main_rs.parent.is_some());

        // Parent should be the src directory
        let parent = tree.get(main_rs.parent.unwrap()).unwrap();
        assert_eq!(parent.name, "src");
    }

    #[test]
    fn test_symbol_nodes() {
        let scan = mock_scan_result();
        let mut builder = TreeBuilder::new();
        let tree = builder.build(&scan);

        // Find the main symbol
        let main_symbol = tree
            .nodes
            .values()
            .find(|n| n.name == "main" && n.is_symbol());
        assert!(main_symbol.is_some());

        let symbol = main_symbol.unwrap();
        assert!(symbol.parent.is_some());

        // Parent should be main.rs
        let parent = tree.get(symbol.parent.unwrap()).unwrap();
        assert_eq!(parent.name, "main.rs");
    }

    #[test]
    fn test_empty_scan() {
        let scan = ScanResult {
            root: PathBuf::from("/empty"),
            files: vec![],
            languages: vec![],
            frameworks: vec![],
            duration_ms: 0,
            skipped_count: 0,
        };

        let mut builder = TreeBuilder::new();
        let tree = builder.build(&scan);

        assert_eq!(tree.file_count, 0);
        assert_eq!(tree.symbol_count, 0);
        assert_eq!(tree.nodes.len(), 1); // Just root
    }

    #[test]
    fn test_deeply_nested_files() {
        let scan = ScanResult {
            root: PathBuf::from("/project"),
            files: vec![ScannedFile {
                path: PathBuf::from("a/b/c/d/file.rs"),
                language: Some(Language::Rust),
                size: 50,
                hash: "xyz".to_string(),
                line_count: 5,
                symbols: vec![],
            }],
            languages: vec![Language::Rust],
            frameworks: vec![],
            duration_ms: 10,
            skipped_count: 0,
        };

        let mut builder = TreeBuilder::new();
        let tree = builder.build(&scan);

        // Should have created all intermediate directories: a, b, c, d
        let dir_count = tree.nodes.values().filter(|n| n.is_directory()).count();
        assert_eq!(dir_count, 5); // root + a + b + c + d
    }
}
