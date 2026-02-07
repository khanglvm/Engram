//! File system walker with gitignore support.

use crate::IndexerError;
use ignore::{WalkBuilder, WalkState};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tracing::debug;

/// A discovered file entry.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Absolute path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modified time (Unix timestamp)
    pub mtime: u64,
}

/// File system walker that respects .gitignore rules.
pub struct Walker {
    root: PathBuf,
    follow_symlinks: bool,
}

impl Walker {
    /// Create a new walker for the given root directory.
    pub fn new(root: &Path, follow_symlinks: bool) -> Self {
        Self {
            root: root.to_path_buf(),
            follow_symlinks,
        }
    }

    /// Walk the directory tree and return all discovered files.
    pub fn walk(&self) -> Result<Vec<FileEntry>, IndexerError> {
        let (tx, rx) = mpsc::channel();

        let walker = WalkBuilder::new(&self.root)
            .follow_links(self.follow_symlinks)
            .hidden(true) // Skip hidden files by default
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(true)
            .parents(true)
            .build_parallel();

        walker.run(|| {
            let tx = tx.clone();
            Box::new(move |result| {
                match result {
                    Ok(entry) => {
                        // Only process files, not directories
                        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                            if let Ok(metadata) = entry.metadata() {
                                let mtime = metadata
                                    .modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);

                                let file_entry = FileEntry {
                                    path: entry.path().to_path_buf(),
                                    size: metadata.len(),
                                    mtime,
                                };

                                let _ = tx.send(Ok(file_entry));
                            }
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, "Walk error");
                        // Don't fail the entire walk for individual errors
                    }
                }
                WalkState::Continue
            })
        });

        // Drop the original sender so the receiver knows when we're done
        drop(tx);

        // Collect results
        let mut entries = Vec::new();
        for result in rx {
            match result {
                Ok(entry) => entries.push(entry),
                Err(e) => return Err(e),
            }
        }

        // Sort by path for deterministic ordering
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_walker_empty_directory() {
        let temp_dir = tempdir().unwrap();
        let walker = Walker::new(temp_dir.path(), false);

        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_walker_with_files() {
        let temp_dir = tempdir().unwrap();

        File::create(temp_dir.path().join("file1.txt")).unwrap();
        File::create(temp_dir.path().join("file2.txt")).unwrap();

        let walker = Walker::new(temp_dir.path(), false);
        let entries = walker.walk().unwrap();

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_walker_respects_gitignore() {
        let temp_dir = tempdir().unwrap();

        // Initialize git repo so .gitignore is recognized
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Create .gitignore first - ignore the 'build' directory
        fs::write(temp_dir.path().join(".gitignore"), "build/\n").unwrap();

        // Create a build directory with files (should be ignored)
        fs::create_dir(temp_dir.path().join("build")).unwrap();
        File::create(temp_dir.path().join("build/output.txt")).unwrap();

        // Create a kept file in root
        File::create(temp_dir.path().join("kept.txt")).unwrap();

        let walker = Walker::new(temp_dir.path(), false);
        let entries = walker.walk().unwrap();

        let paths: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.file_name().and_then(|n| n.to_str()))
            .collect();

        // Should find kept.txt
        assert!(
            paths.contains(&"kept.txt"),
            "Should find kept.txt, found: {:?}",
            paths
        );
        // Should NOT find files in build/ directory
        assert!(
            !paths.contains(&"output.txt"),
            "Should not find output.txt in build/, found: {:?}",
            paths
        );
    }

    #[test]
    fn test_walker_skips_hidden_files() {
        let temp_dir = tempdir().unwrap();

        File::create(temp_dir.path().join("visible.txt")).unwrap();
        File::create(temp_dir.path().join(".hidden.txt")).unwrap();

        let walker = Walker::new(temp_dir.path(), false);
        let entries = walker.walk().unwrap();

        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(paths.contains(&"visible.txt"));
        assert!(!paths.contains(&".hidden.txt"));
    }

    #[test]
    fn test_walker_handles_nested_directories() {
        let temp_dir = tempdir().unwrap();

        fs::create_dir_all(temp_dir.path().join("a/b/c")).unwrap();
        File::create(temp_dir.path().join("a/file1.txt")).unwrap();
        File::create(temp_dir.path().join("a/b/file2.txt")).unwrap();
        File::create(temp_dir.path().join("a/b/c/file3.txt")).unwrap();

        let walker = Walker::new(temp_dir.path(), false);
        let entries = walker.walk().unwrap();

        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_walker_file_entry_has_metadata() {
        let temp_dir = tempdir().unwrap();

        let content = "hello world";
        fs::write(temp_dir.path().join("test.txt"), content).unwrap();

        let walker = Walker::new(temp_dir.path(), false);
        let entries = walker.walk().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, content.len() as u64);
        assert!(entries[0].mtime > 0);
    }

    #[test]
    fn test_walker_results_are_sorted() {
        let temp_dir = tempdir().unwrap();

        File::create(temp_dir.path().join("c.txt")).unwrap();
        File::create(temp_dir.path().join("a.txt")).unwrap();
        File::create(temp_dir.path().join("b.txt")).unwrap();

        let walker = Walker::new(temp_dir.path(), false);
        let entries = walker.walk().unwrap();

        let names: Vec<_> = entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_str().unwrap())
            .collect();

        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
    }
}
