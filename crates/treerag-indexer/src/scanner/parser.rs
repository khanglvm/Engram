//! AST parsing with tree-sitter.

use super::Language;
use crate::IndexerError;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// A parsed file with extracted symbols.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    /// Extracted symbols
    pub symbols: Vec<Symbol>,
}

/// A code symbol (function, class, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Kind of symbol
    pub kind: SymbolKind,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Parent symbol name (for nested symbols)
    pub parent: Option<String>,
    /// Brief documentation/comment if present
    pub doc: Option<String>,
}

/// Kind of symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Module,
    Constant,
    Variable,
    Import,
}

/// AST parser using tree-sitter.
pub struct Parser {
    // Tree-sitter parsers are created on-demand per language
}

impl Parser {
    /// Create a new parser.
    pub fn new() -> Self {
        Self {}
    }

    /// Parse source code and extract symbols.
    pub fn parse(&self, content: &str, language: &Language) -> Result<ParsedFile, IndexerError> {
        if !language.has_parser() {
            return Ok(ParsedFile { symbols: vec![] });
        }

        let mut parser = tree_sitter::Parser::new();

        // Get the language grammar
        let ts_language = match language {
            Language::Rust => tree_sitter_rust::LANGUAGE,
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            Language::JavaScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT, // TS parser handles JS
            Language::Python => tree_sitter_python::LANGUAGE,
            Language::Go => tree_sitter_go::LANGUAGE,
            _ => return Ok(ParsedFile { symbols: vec![] }),
        };

        parser
            .set_language(&ts_language.into())
            .map_err(|e| IndexerError::Parse {
                path: std::path::PathBuf::new(),
                message: format!("Failed to set language: {}", e),
            })?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| IndexerError::Parse {
                path: std::path::PathBuf::new(),
                message: "Failed to parse content".to_string(),
            })?;

        let symbols = extract_symbols(&tree, content, language);

        debug!(symbol_count = symbols.len(), "Extracted symbols");

        Ok(ParsedFile { symbols })
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract symbols from a parsed tree.
fn extract_symbols(tree: &tree_sitter::Tree, content: &str, language: &Language) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let root = tree.root_node();

    extract_symbols_recursive(root, content, language, None, &mut symbols);

    symbols
}

fn extract_symbols_recursive(
    node: tree_sitter::Node,
    content: &str,
    language: &Language,
    parent: Option<String>,
    symbols: &mut Vec<Symbol>,
) {
    let kind = node.kind();

    // Map node kinds to symbol kinds based on language
    let symbol_kind = match (language, kind) {
        // Rust
        (Language::Rust, "function_item") => Some(SymbolKind::Function),
        (Language::Rust, "impl_item") => Some(SymbolKind::Module), // Treat impl as module for grouping
        (Language::Rust, "struct_item") => Some(SymbolKind::Struct),
        (Language::Rust, "enum_item") => Some(SymbolKind::Enum),
        (Language::Rust, "trait_item") => Some(SymbolKind::Trait),
        (Language::Rust, "mod_item") => Some(SymbolKind::Module),
        (Language::Rust, "const_item") => Some(SymbolKind::Constant),
        (Language::Rust, "static_item") => Some(SymbolKind::Constant),

        // TypeScript/JavaScript
        (Language::TypeScript | Language::JavaScript, "function_declaration") => {
            Some(SymbolKind::Function)
        }
        (Language::TypeScript | Language::JavaScript, "method_definition") => {
            Some(SymbolKind::Method)
        }
        (Language::TypeScript | Language::JavaScript, "class_declaration") => {
            Some(SymbolKind::Class)
        }
        (Language::TypeScript | Language::JavaScript, "interface_declaration") => {
            Some(SymbolKind::Interface)
        }
        (Language::TypeScript | Language::JavaScript, "type_alias_declaration") => {
            Some(SymbolKind::Interface)
        }
        (Language::TypeScript | Language::JavaScript, "arrow_function") => None, // Skip anonymous

        // Python
        (Language::Python, "function_definition") => Some(SymbolKind::Function),
        (Language::Python, "class_definition") => Some(SymbolKind::Class),

        // Go
        (Language::Go, "function_declaration") => Some(SymbolKind::Function),
        (Language::Go, "method_declaration") => Some(SymbolKind::Method),
        (Language::Go, "type_declaration") => None, // Handle nested type_spec
        (Language::Go, "type_spec") => Some(SymbolKind::Struct),

        _ => None,
    };

    if let Some(sk) = symbol_kind {
        // Try to extract the name
        if let Some(name) = extract_name(node, content, language) {
            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;

            symbols.push(Symbol {
                name: name.clone(),
                kind: sk,
                start_line,
                end_line,
                parent: parent.clone(),
                doc: extract_doc_comment(node, content),
            });

            // Recurse with this symbol as parent for nested items
            for child in node.children(&mut node.walk()) {
                extract_symbols_recursive(child, content, language, Some(name.clone()), symbols);
            }
            return;
        }
    }

    // Recurse for non-symbol nodes
    for child in node.children(&mut node.walk()) {
        extract_symbols_recursive(child, content, language, parent.clone(), symbols);
    }
}

/// Extract the name of a symbol node.
fn extract_name(node: tree_sitter::Node, content: &str, _language: &Language) -> Option<String> {
    // Look for 'name' or 'identifier' child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if kind == "name" || kind == "identifier" || kind == "type_identifier" {
            let start = child.start_byte();
            let end = child.end_byte();
            if let Some(text) = content.get(start..end) {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Extract documentation comment above a node.
fn extract_doc_comment(node: tree_sitter::Node, content: &str) -> Option<String> {
    // Look for preceding comment siblings
    if let Some(prev) = node.prev_sibling() {
        let kind = prev.kind();
        if kind == "comment" || kind == "line_comment" || kind == "block_comment" {
            let start = prev.start_byte();
            let end = prev.end_byte();
            if let Some(text) = content.get(start..end) {
                // Clean up comment syntax
                let cleaned = text
                    .lines()
                    .map(|l| l.trim_start_matches("///").trim_start_matches("//").trim())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_function() {
        let parser = Parser::new();
        let code = r#"
/// This is a test function
fn hello_world() {
    println!("Hello!");
}
"#;
        let result = parser.parse(code, &Language::Rust).unwrap();

        assert!(!result.symbols.is_empty());
        let func = &result.symbols[0];
        assert_eq!(func.name, "hello_world");
        assert_eq!(func.kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_rust_struct() {
        let parser = Parser::new();
        let code = r#"
struct Point {
    x: f64,
    y: f64,
}
"#;
        let result = parser.parse(code, &Language::Rust).unwrap();

        assert!(!result.symbols.is_empty());
        let s = result
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Struct)
            .unwrap();
        assert_eq!(s.name, "Point");
    }

    #[test]
    fn test_parse_typescript_class() {
        let parser = Parser::new();
        let code = r#"
class MyClass {
    constructor() {}
    
    myMethod() {
        return 42;
    }
}
"#;
        let result = parser.parse(code, &Language::TypeScript).unwrap();

        let class = result.symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(class.is_some());
        assert_eq!(class.unwrap().name, "MyClass");
    }

    #[test]
    fn test_parse_python_function() {
        let parser = Parser::new();
        let code = r#"
def greet(name):
    print(f"Hello, {name}!")
"#;
        let result = parser.parse(code, &Language::Python).unwrap();

        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "greet");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_go_function() {
        let parser = Parser::new();
        let code = r#"
func main() {
    fmt.Println("Hello")
}
"#;
        let result = parser.parse(code, &Language::Go).unwrap();

        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "main");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_unsupported_language() {
        let parser = Parser::new();
        let result = parser.parse("{}", &Language::Json).unwrap();

        assert!(result.symbols.is_empty());
    }

    #[test]
    fn test_symbol_line_numbers() {
        let parser = Parser::new();
        let code = "fn foo() {\n    // body\n}\n";
        let result = parser.parse(code, &Language::Rust).unwrap();

        let func = &result.symbols[0];
        assert_eq!(func.start_line, 1);
        assert_eq!(func.end_line, 3);
    }
}
