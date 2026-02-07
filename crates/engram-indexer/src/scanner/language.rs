//! Language detection for source files.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Json,
    Yaml,
    Toml,
    Markdown,
    Html,
    Css,
    Unknown,
}

impl Language {
    /// Get the display name for this language.
    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Python => "Python",
            Language::Go => "Go",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Unknown => "Unknown",
        }
    }

    /// Check if this language has tree-sitter support.
    pub fn has_parser(&self) -> bool {
        matches!(
            self,
            Language::Rust
                | Language::TypeScript
                | Language::JavaScript
                | Language::Python
                | Language::Go
        )
    }
}

/// Detect the language of a file based on its extension.
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?.to_lowercase();

    match ext.as_str() {
        // Rust
        "rs" => Some(Language::Rust),

        // TypeScript/JavaScript
        "ts" | "tsx" => Some(Language::TypeScript),
        "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),

        // Python
        "py" | "pyi" | "pyw" => Some(Language::Python),

        // Go
        "go" => Some(Language::Go),

        // Config/Data
        "json" => Some(Language::Json),
        "yaml" | "yml" => Some(Language::Yaml),
        "toml" => Some(Language::Toml),

        // Web
        "html" | "htm" => Some(Language::Html),
        "css" | "scss" | "sass" => Some(Language::Css),

        // Documentation
        "md" | "markdown" => Some(Language::Markdown),

        _ => None,
    }
}

/// Detect language from file content (magic bytes or shebang).
pub fn detect_language_from_content(content: &str) -> Option<Language> {
    let first_line = content.lines().next()?;

    // Check for shebang
    if first_line.starts_with("#!") {
        if first_line.contains("python") {
            return Some(Language::Python);
        }
        if first_line.contains("node") || first_line.contains("deno") || first_line.contains("bun")
        {
            return Some(Language::JavaScript);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_rust() {
        assert_eq!(
            detect_language(&PathBuf::from("main.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            detect_language(&PathBuf::from("lib.rs")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn test_detect_typescript() {
        assert_eq!(
            detect_language(&PathBuf::from("index.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            detect_language(&PathBuf::from("App.tsx")),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn test_detect_javascript() {
        assert_eq!(
            detect_language(&PathBuf::from("index.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            detect_language(&PathBuf::from("App.jsx")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            detect_language(&PathBuf::from("module.mjs")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            detect_language(&PathBuf::from("common.cjs")),
            Some(Language::JavaScript)
        );
    }

    #[test]
    fn test_detect_python() {
        assert_eq!(
            detect_language(&PathBuf::from("main.py")),
            Some(Language::Python)
        );
        assert_eq!(
            detect_language(&PathBuf::from("types.pyi")),
            Some(Language::Python)
        );
    }

    #[test]
    fn test_detect_go() {
        assert_eq!(
            detect_language(&PathBuf::from("main.go")),
            Some(Language::Go)
        );
    }

    #[test]
    fn test_detect_config_files() {
        assert_eq!(
            detect_language(&PathBuf::from("package.json")),
            Some(Language::Json)
        );
        assert_eq!(
            detect_language(&PathBuf::from("config.yaml")),
            Some(Language::Yaml)
        );
        assert_eq!(
            detect_language(&PathBuf::from("config.yml")),
            Some(Language::Yaml)
        );
        assert_eq!(
            detect_language(&PathBuf::from("Cargo.toml")),
            Some(Language::Toml)
        );
    }

    #[test]
    fn test_detect_web_files() {
        assert_eq!(
            detect_language(&PathBuf::from("index.html")),
            Some(Language::Html)
        );
        assert_eq!(
            detect_language(&PathBuf::from("styles.css")),
            Some(Language::Css)
        );
    }

    #[test]
    fn test_detect_markdown() {
        assert_eq!(
            detect_language(&PathBuf::from("README.md")),
            Some(Language::Markdown)
        );
        assert_eq!(
            detect_language(&PathBuf::from("docs.markdown")),
            Some(Language::Markdown)
        );
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_language(&PathBuf::from("file.xyz")), None);
        assert_eq!(detect_language(&PathBuf::from("noextension")), None);
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(
            detect_language(&PathBuf::from("main.RS")),
            Some(Language::Rust)
        );
        assert_eq!(
            detect_language(&PathBuf::from("index.TS")),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn test_language_name() {
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::TypeScript.name(), "TypeScript");
    }

    #[test]
    fn test_has_parser() {
        assert!(Language::Rust.has_parser());
        assert!(Language::TypeScript.has_parser());
        assert!(Language::Python.has_parser());
        assert!(Language::Go.has_parser());
        assert!(!Language::Json.has_parser());
        assert!(!Language::Markdown.has_parser());
    }

    #[test]
    fn test_detect_from_shebang() {
        assert_eq!(
            detect_language_from_content("#!/usr/bin/env python3\nprint('hello')"),
            Some(Language::Python)
        );
        assert_eq!(
            detect_language_from_content("#!/usr/bin/node\nconsole.log('hi')"),
            Some(Language::JavaScript)
        );
        assert_eq!(detect_language_from_content("no shebang"), None);
    }
}
