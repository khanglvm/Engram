//! Framework detection from configuration files.

use crate::IndexerError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;

/// Detected framework or technology.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Framework {
    /// Framework name
    pub name: String,
    /// Category (e.g., "web", "cli", "library")
    pub category: String,
}

/// Detect frameworks in a project by examining configuration files.
pub async fn detect_frameworks(root: &Path) -> Result<Vec<Framework>, IndexerError> {
    let mut frameworks = Vec::new();

    // Check for various framework indicators

    // JavaScript/TypeScript frameworks
    if let Ok(content) = tokio::fs::read_to_string(root.join("package.json")).await {
        detect_js_frameworks(&content, &mut frameworks);
    }

    // Python frameworks
    if let Ok(content) = tokio::fs::read_to_string(root.join("pyproject.toml")).await {
        detect_python_frameworks(&content, &mut frameworks);
    }
    if let Ok(content) = tokio::fs::read_to_string(root.join("requirements.txt")).await {
        detect_python_from_requirements(&content, &mut frameworks);
    }

    // Rust frameworks
    if let Ok(content) = tokio::fs::read_to_string(root.join("Cargo.toml")).await {
        detect_rust_frameworks(&content, &mut frameworks);
    }

    // Go frameworks
    if let Ok(content) = tokio::fs::read_to_string(root.join("go.mod")).await {
        detect_go_frameworks(&content, &mut frameworks);
    }

    // Docker
    if root.join("Dockerfile").exists() || root.join("docker-compose.yml").exists() {
        frameworks.push(Framework {
            name: "Docker".to_string(),
            category: "infrastructure".to_string(),
        });
    }

    debug!(count = frameworks.len(), "Detected frameworks");

    Ok(frameworks)
}

fn detect_js_frameworks(content: &str, frameworks: &mut Vec<Framework>) {
    let content_lower = content.to_lowercase();

    // React
    if content_lower.contains("\"react\"") || content_lower.contains("'react'") {
        frameworks.push(Framework {
            name: "React".to_string(),
            category: "frontend".to_string(),
        });
    }

    // Next.js
    if content_lower.contains("\"next\"") || content_lower.contains("'next'") {
        frameworks.push(Framework {
            name: "Next.js".to_string(),
            category: "fullstack".to_string(),
        });
    }

    // Vue
    if content_lower.contains("\"vue\"") || content_lower.contains("'vue'") {
        frameworks.push(Framework {
            name: "Vue".to_string(),
            category: "frontend".to_string(),
        });
    }

    // Express
    if content_lower.contains("\"express\"") || content_lower.contains("'express'") {
        frameworks.push(Framework {
            name: "Express".to_string(),
            category: "backend".to_string(),
        });
    }

    // Vite
    if content_lower.contains("\"vite\"") || content_lower.contains("'vite'") {
        frameworks.push(Framework {
            name: "Vite".to_string(),
            category: "build".to_string(),
        });
    }

    // TypeScript
    if content_lower.contains("\"typescript\"") || content_lower.contains("'typescript'") {
        frameworks.push(Framework {
            name: "TypeScript".to_string(),
            category: "language".to_string(),
        });
    }

    // Tailwind
    if content_lower.contains("\"tailwindcss\"") || content_lower.contains("'tailwindcss'") {
        frameworks.push(Framework {
            name: "Tailwind CSS".to_string(),
            category: "styling".to_string(),
        });
    }
}

fn detect_python_frameworks(content: &str, frameworks: &mut Vec<Framework>) {
    let content_lower = content.to_lowercase();

    // Django
    if content_lower.contains("django") {
        frameworks.push(Framework {
            name: "Django".to_string(),
            category: "fullstack".to_string(),
        });
    }

    // FastAPI
    if content_lower.contains("fastapi") {
        frameworks.push(Framework {
            name: "FastAPI".to_string(),
            category: "backend".to_string(),
        });
    }

    // Flask
    if content_lower.contains("flask") {
        frameworks.push(Framework {
            name: "Flask".to_string(),
            category: "backend".to_string(),
        });
    }
}

fn detect_python_from_requirements(content: &str, frameworks: &mut Vec<Framework>) {
    let content_lower = content.to_lowercase();

    // Same checks as pyproject.toml
    if content_lower.contains("django") {
        frameworks.push(Framework {
            name: "Django".to_string(),
            category: "fullstack".to_string(),
        });
    }
    if content_lower.contains("fastapi") {
        frameworks.push(Framework {
            name: "FastAPI".to_string(),
            category: "backend".to_string(),
        });
    }
    if content_lower.contains("flask") {
        frameworks.push(Framework {
            name: "Flask".to_string(),
            category: "backend".to_string(),
        });
    }
}

fn detect_rust_frameworks(content: &str, frameworks: &mut Vec<Framework>) {
    let content_lower = content.to_lowercase();

    // Tokio
    if content_lower.contains("tokio") {
        frameworks.push(Framework {
            name: "Tokio".to_string(),
            category: "async".to_string(),
        });
    }

    // Axum
    if content_lower.contains("axum") {
        frameworks.push(Framework {
            name: "Axum".to_string(),
            category: "backend".to_string(),
        });
    }

    // Actix
    if content_lower.contains("actix-web") || content_lower.contains("actix_web") {
        frameworks.push(Framework {
            name: "Actix Web".to_string(),
            category: "backend".to_string(),
        });
    }

    // Serde
    if content_lower.contains("serde") {
        frameworks.push(Framework {
            name: "Serde".to_string(),
            category: "serialization".to_string(),
        });
    }

    // Clap
    if content_lower.contains("clap") {
        frameworks.push(Framework {
            name: "Clap".to_string(),
            category: "cli".to_string(),
        });
    }
}

fn detect_go_frameworks(content: &str, frameworks: &mut Vec<Framework>) {
    // Gin
    if content.contains("github.com/gin-gonic/gin") {
        frameworks.push(Framework {
            name: "Gin".to_string(),
            category: "backend".to_string(),
        });
    }

    // Echo
    if content.contains("github.com/labstack/echo") {
        frameworks.push(Framework {
            name: "Echo".to_string(),
            category: "backend".to_string(),
        });
    }

    // Fiber
    if content.contains("github.com/gofiber/fiber") {
        frameworks.push(Framework {
            name: "Fiber".to_string(),
            category: "backend".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_detect_no_frameworks() {
        let temp_dir = tempdir().unwrap();
        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.is_empty());
    }

    #[tokio::test]
    async fn test_detect_react() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{"dependencies": {"react": "^18.0.0"}}"#,
        )
        .unwrap();

        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.iter().any(|f| f.name == "React"));
    }

    #[tokio::test]
    async fn test_detect_nextjs() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("package.json"),
            r#"{"dependencies": {"next": "^14.0.0", "react": "^18.0.0"}}"#,
        )
        .unwrap();

        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.iter().any(|f| f.name == "Next.js"));
        assert!(frameworks.iter().any(|f| f.name == "React"));
    }

    #[tokio::test]
    async fn test_detect_tokio() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("Cargo.toml"),
            r#"[dependencies]
tokio = { version = "1.0", features = ["full"] }
"#,
        )
        .unwrap();

        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.iter().any(|f| f.name == "Tokio"));
    }

    #[tokio::test]
    async fn test_detect_docker() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("Dockerfile"), "FROM rust:latest").unwrap();

        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.iter().any(|f| f.name == "Docker"));
    }

    #[tokio::test]
    async fn test_detect_fastapi() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("pyproject.toml"),
            r#"[project]
dependencies = ["fastapi>=0.100.0"]
"#,
        )
        .unwrap();

        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.iter().any(|f| f.name == "FastAPI"));
    }

    #[tokio::test]
    async fn test_detect_gin() {
        let temp_dir = tempdir().unwrap();
        fs::write(
            temp_dir.path().join("go.mod"),
            r#"module myapp

require github.com/gin-gonic/gin v1.9.0
"#,
        )
        .unwrap();

        let frameworks = detect_frameworks(temp_dir.path()).await.unwrap();
        assert!(frameworks.iter().any(|f| f.name == "Gin"));
    }

    #[test]
    fn test_framework_serialization() {
        let framework = Framework {
            name: "React".to_string(),
            category: "frontend".to_string(),
        };

        let json = serde_json::to_string(&framework).unwrap();
        let deserialized: Framework = serde_json::from_str(&json).unwrap();

        assert_eq!(framework, deserialized);
    }
}
