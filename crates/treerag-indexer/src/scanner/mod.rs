//! File system scanner module.
//!
//! Provides fast, parallel file scanning with gitignore support,
//! language detection, and AST parsing.

mod framework;
mod language;
mod parser;
mod walker;

pub use framework::{detect_frameworks, Framework};
pub use language::{detect_language, Language};
pub use parser::{ParsedFile, Parser, Symbol, SymbolKind};
pub use walker::{FileEntry, Walker};

use crate::IndexerError;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, warn};

/// Options for scanning a project.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Maximum number of files to scan (0 = unlimited)
    pub max_files: usize,
    /// Maximum file size to parse in bytes (larger files are skipped)
    pub max_file_size: u64,
    /// Whether to follow symlinks
    pub follow_symlinks: bool,
    /// Whether to parse files for symbols
    pub parse_symbols: bool,
    /// Number of parallel threads for walking
    pub parallelism: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_files: 0,                    // unlimited
            max_file_size: 10 * 1024 * 1024, // 10MB
            follow_symlinks: false,
            parse_symbols: true,
            parallelism: num_cpus(),
        }
    }
}

/// Result of scanning a project.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Root path that was scanned
    pub root: PathBuf,
    /// All discovered files
    pub files: Vec<ScannedFile>,
    /// Detected languages
    pub languages: Vec<Language>,
    /// Detected frameworks
    pub frameworks: Vec<Framework>,
    /// Scan duration in milliseconds
    pub duration_ms: u64,
    /// Number of files skipped (errors, too large, etc.)
    pub skipped_count: usize,
}

/// A scanned file with its metadata and parsed content.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Relative path from scan root
    pub path: PathBuf,
    /// Detected language
    pub language: Option<Language>,
    /// File size in bytes
    pub size: u64,
    /// Content hash (SHA256)
    pub hash: String,
    /// Line count
    pub line_count: usize,
    /// Extracted symbols (if parsing enabled)
    pub symbols: Vec<Symbol>,
}

/// The main scanner that orchestrates file discovery and parsing.
pub struct Scanner {
    options: ScanOptions,
}

impl Scanner {
    /// Create a new scanner with default options.
    pub fn new() -> Self {
        Self {
            options: ScanOptions::default(),
        }
    }

    /// Create a scanner with custom options.
    pub fn with_options(options: ScanOptions) -> Self {
        Self { options }
    }

    /// Scan a directory and return results.
    pub async fn scan(&self, root: &Path) -> Result<ScanResult, IndexerError> {
        let start = Instant::now();

        let root = root
            .canonicalize()
            .map_err(|e| IndexerError::NotFound(root.to_path_buf()))?;

        info!(path = ?root, "Starting scan");

        // Step 1: Walk the file system
        let walker = Walker::new(&root, self.options.follow_symlinks);
        let entries = walker.walk()?;

        debug!(count = entries.len(), "Files discovered");

        // Apply max_files limit if set
        let entries: Vec<_> = if self.options.max_files > 0 {
            entries.into_iter().take(self.options.max_files).collect()
        } else {
            entries
        };

        // Step 2: Process files (detect language, parse, hash)
        let mut files = Vec::with_capacity(entries.len());
        let mut skipped = 0;
        let mut language_set = std::collections::HashSet::new();

        let parser = Parser::new();

        for entry in entries {
            // Skip files that are too large
            if entry.size > self.options.max_file_size {
                debug!(path = ?entry.path, size = entry.size, "Skipping large file");
                skipped += 1;
                continue;
            }

            let rel_path = entry
                .path
                .strip_prefix(&root)
                .unwrap_or(&entry.path)
                .to_path_buf();

            let language = detect_language(&entry.path);

            if let Some(lang) = &language {
                language_set.insert(lang.clone());
            }

            // Read file content for hashing and parsing
            let content = match tokio::fs::read_to_string(&entry.path).await {
                Ok(c) => c,
                Err(e) => {
                    debug!(path = ?entry.path, error = %e, "Failed to read file");
                    skipped += 1;
                    continue;
                }
            };

            let hash = compute_hash(&content);
            let line_count = content.lines().count();

            // Parse symbols if enabled and language is supported
            let symbols = if self.options.parse_symbols {
                if let Some(lang) = &language {
                    match parser.parse(&content, lang) {
                        Ok(parsed) => parsed.symbols,
                        Err(e) => {
                            warn!(path = ?entry.path, error = %e, "Parse failed");
                            vec![]
                        }
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            files.push(ScannedFile {
                path: rel_path,
                language,
                size: entry.size,
                hash,
                line_count,
                symbols,
            });
        }

        // Step 3: Detect frameworks
        let frameworks = detect_frameworks(&root).await?;

        let duration = start.elapsed();

        info!(
            files = files.len(),
            skipped = skipped,
            languages = language_set.len(),
            frameworks = frameworks.len(),
            duration_ms = duration.as_millis(),
            "Scan complete"
        );

        Ok(ScanResult {
            root,
            files,
            languages: language_set.into_iter().collect(),
            frameworks,
            duration_ms: duration.as_millis() as u64,
            skipped_count: skipped,
        })
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute SHA256 hash of content.
fn compute_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Get the number of CPUs available.
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let temp_dir = tempdir().unwrap();
        let scanner = Scanner::new();

        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.files.len(), 0);
        assert_eq!(result.skipped_count, 0);
    }

    #[tokio::test]
    async fn test_scan_with_files() {
        let temp_dir = tempdir().unwrap();

        // Create test files
        fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(temp_dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
        fs::write(temp_dir.path().join("README.md"), "# Test").unwrap();

        let scanner = Scanner::new();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.files.len(), 3);
        assert!(result.languages.contains(&Language::Rust));
    }

    #[tokio::test]
    async fn test_scan_respects_gitignore() {
        let temp_dir = tempdir().unwrap();

        // Initialize git repo so .gitignore is recognized
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Create .gitignore first - ignore 'build' directory
        fs::write(temp_dir.path().join(".gitignore"), "build/\n").unwrap();

        // Create a build directory with files (should be ignored)
        fs::create_dir(temp_dir.path().join("build")).unwrap();
        fs::write(temp_dir.path().join("build/output.rs"), "// output").unwrap();

        // Create kept files
        fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();

        let scanner = Scanner::new();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        let paths: Vec<_> = result
            .files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();

        // Should find main.rs
        assert!(
            paths.contains(&"main.rs".to_string()),
            "Should find main.rs, found: {:?}",
            paths
        );
        // Should NOT find files in build/ directory
        assert!(
            !paths.iter().any(|p| p.contains("build")),
            "Should not find build/, found: {:?}",
            paths
        );
    }

    #[test]
    fn test_compute_hash() {
        let hash1 = compute_hash("hello world");
        let hash2 = compute_hash("hello world");
        let hash3 = compute_hash("different");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA256 hex length
    }

    #[test]
    fn test_scan_options_default() {
        let opts = ScanOptions::default();
        assert_eq!(opts.max_files, 0);
        assert_eq!(opts.max_file_size, 10 * 1024 * 1024);
        assert!(!opts.follow_symlinks);
        assert!(opts.parse_symbols);
    }
}
