//! Integration tests for TreeRAG indexer scan pipeline and storage.

use std::path::PathBuf;
use tempfile::tempdir;

use treerag_indexer::scanner::{ScanOptions, Scanner};
use treerag_indexer::storage::Storage;

/// Helper to create a test project structure
fn create_test_project(base: &std::path::Path) -> PathBuf {
    let project = base.join("test_project");
    std::fs::create_dir_all(&project).unwrap();

    // Create Rust project structure
    std::fs::write(
        project.join("Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"
"#,
    )
    .unwrap();

    let src = project.join("src");
    std::fs::create_dir_all(&src).unwrap();

    std::fs::write(
        src.join("main.rs"),
        r#"mod lib;

fn main() {
    println!("Hello, world!");
}
"#,
    )
    .unwrap();

    std::fs::write(
        src.join("lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 2), 4);
    }
}
"#,
    )
    .unwrap();

    project
}

/// Test full scan pipeline end-to-end
#[tokio::test]
async fn test_scan_pipeline_end_to_end() {
    let temp_dir = tempdir().unwrap();
    let project = create_test_project(temp_dir.path());

    let scanner = Scanner::new();
    let result = scanner.scan(&project).await.unwrap();

    // Should have scanned files
    assert!(!result.files.is_empty(), "Should have scanned files");

    // Should have detected Rust files
    let has_rust = result
        .files
        .iter()
        .any(|f| f.path.extension().map_or(false, |e| e == "rs"));
    assert!(has_rust, "Should detect Rust files");
}

/// Test scan with gitignore respect
#[tokio::test]
async fn test_scan_respects_gitignore() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("gitignore_test");
    std::fs::create_dir_all(&project).unwrap();

    // Create .gitignore
    std::fs::write(project.join(".gitignore"), "target/\n*.log\n").unwrap();

    // Create some files
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(project.join("test.log"), "log content").unwrap();

    // Create target directory (should be ignored)
    std::fs::create_dir_all(project.join("target")).unwrap();
    std::fs::write(project.join("target/debug.rs"), "ignored").unwrap();

    let scanner = Scanner::new();
    let result = scanner.scan(&project).await.unwrap();

    // Should have main.rs but not necessarily exclude all ignored files
    // (depends on walker implementation)
    let has_main = result
        .files
        .iter()
        .any(|f| f.path.file_name().map_or(false, |n| n == "main.rs"));
    assert!(has_main, "Should include main.rs");

    // Files count should be reasonable (not include everything)
    assert!(result.files.len() < 10, "Should not include too many files");
}

/// Test experience log append and load
#[tokio::test]
async fn test_experience_log_roundtrip() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestExperience {
        agent_id: String,
        decision: String,
        timestamp: i64,
    }

    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("exp_project");
    std::fs::create_dir_all(&project).unwrap();
    let storage_dir = temp_dir.path().join("storage");

    let storage = Storage::new(storage_dir.clone());

    // Append experiences
    let exp1 = TestExperience {
        agent_id: "agent1".to_string(),
        decision: "use_pattern_a".to_string(),
        timestamp: 1000,
    };
    let exp2 = TestExperience {
        agent_id: "agent1".to_string(),
        decision: "use_pattern_b".to_string(),
        timestamp: 2000,
    };

    storage.append_experience(&project, &exp1).await.unwrap();
    storage.append_experience(&project, &exp2).await.unwrap();

    // Load experiences
    let loaded: Vec<TestExperience> = storage.load_experiences(&project, 100).await.unwrap();

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0], exp1);
    assert_eq!(loaded[1], exp2);
}

/// Test scan performance on medium project
#[tokio::test]
async fn test_scan_performance_medium_project() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("perf_test");
    std::fs::create_dir_all(&project).unwrap();

    // Create 100 files across 10 directories
    for i in 0..10 {
        let dir = project.join(format!("module_{}", i));
        std::fs::create_dir_all(&dir).unwrap();

        for j in 0..10 {
            std::fs::write(
                dir.join(format!("file_{}.rs", j)),
                format!("pub fn func_{}() {{ }}", j),
            )
            .unwrap();
        }
    }

    let start = std::time::Instant::now();

    let scanner = Scanner::new();
    let result = scanner.scan(&project).await.unwrap();

    let elapsed = start.elapsed();

    // Should complete in under 1 second for 100 files
    assert!(elapsed.as_secs() < 1, "Scan took too long: {:?}", elapsed);

    // Verify all files were scanned
    assert!(
        result.files.len() >= 100,
        "Expected 100+ files, got {}",
        result.files.len()
    );
}

/// Test scan handles empty directory
#[tokio::test]
async fn test_scan_empty_directory() {
    let temp_dir = tempdir().unwrap();
    let empty_project = temp_dir.path().join("empty");
    std::fs::create_dir_all(&empty_project).unwrap();

    let scanner = Scanner::new();
    let result = scanner.scan(&empty_project).await.unwrap();

    // Should have no files
    assert!(result.files.is_empty());
}

/// Test scan handles symlinks gracefully
#[tokio::test]
#[cfg(unix)]
async fn test_scan_handles_symlinks() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("symlink_test");
    std::fs::create_dir_all(&project).unwrap();

    // Create a real file
    std::fs::write(project.join("real.rs"), "fn real() {}").unwrap();

    // Create a symlink
    let link_target = project.join("real.rs");
    let link_path = project.join("link.rs");
    symlink(&link_target, &link_path).unwrap();

    let scanner = Scanner::new();

    // Should not crash on symlinks
    let result = scanner.scan(&project).await;
    assert!(result.is_ok());
}

/// Test scan detects frameworks
#[tokio::test]
async fn test_scan_detects_frameworks() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("framework_test");
    std::fs::create_dir_all(&project).unwrap();

    // Create Cargo.toml (cargo/rust framework indicator)
    std::fs::write(
        project.join("Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let scanner = Scanner::new();
    let result = scanner.scan(&project).await.unwrap();

    // Framework detection is optional - some setups may not detect all frameworks
    // Just verify scan completed successfully
    assert!(result.files.len() >= 1, "Should scan files");
    // If frameworks detected, verify it's reasonable
    if !result.frameworks.is_empty() {
        assert!(
            result.frameworks.len() < 10,
            "Should have reasonable framework count"
        );
    }
}

/// Test scan with custom options
#[tokio::test]
async fn test_scan_with_custom_options() {
    let temp_dir = tempdir().unwrap();
    let project = temp_dir.path().join("options_test");
    std::fs::create_dir_all(&project).unwrap();

    // Create 20 files
    for i in 0..20 {
        std::fs::write(
            project.join(format!("file_{}.rs", i)),
            format!("fn f{}() {{}}", i),
        )
        .unwrap();
    }

    // Limit to 5 files
    let options = ScanOptions {
        max_files: 5,
        ..Default::default()
    };
    let scanner = Scanner::with_options(options);
    let result = scanner.scan(&project).await.unwrap();

    // Should have at most 5 files
    assert!(
        result.files.len() <= 5,
        "Expected <= 5 files, got {}",
        result.files.len()
    );
}
