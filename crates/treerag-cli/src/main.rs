//! TreeRAG CLI
//!
//! Command-line interface for managing the TreeRAG daemon and projects.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use treerag_ipc::{IpcClient, Request, Response, ResponseData};

#[derive(Parser)]
#[command(name = "treerag")]
#[command(about = "TreeRAG - Smart context management for AI coding assistants")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the TreeRAG daemon
    Start {
        /// Run in foreground (for debugging)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop the TreeRAG daemon
    Stop,

    /// Show daemon status
    Status,

    /// Initialize a project for TreeRAG
    Init {
        /// Project path (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Skip AI enrichment (fast mode)
        #[arg(long)]
        quick: bool,
    },

    /// Show project information
    Project {
        /// Project path (default: current directory)
        #[arg(default_value = ".")]
        path: String,
    },

    /// Check if daemon is running
    Ping,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Simple logging for CLI
    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt().with_target(false).init();
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { foreground } => cmd_start(foreground).await,
        Commands::Stop => cmd_stop().await,
        Commands::Status => cmd_status().await,
        Commands::Init { path, quick } => cmd_init(&path, quick).await,
        Commands::Project { path } => cmd_project(&path).await,
        Commands::Ping => cmd_ping().await,
    }
}

async fn cmd_start(foreground: bool) -> Result<()> {
    if foreground {
        println!("Starting TreeRAG daemon in foreground...");
        println!("Press Ctrl+C to stop.");

        // Execute daemon directly
        let status = std::process::Command::new("treerag-daemon")
            .status()
            .context("Failed to start daemon. Is treerag-daemon in PATH?")?;

        if !status.success() {
            anyhow::bail!("Daemon exited with error");
        }
    } else {
        // Check if already running
        if IpcClient::new().is_daemon_running() {
            println!("TreeRAG daemon is already running.");
            return Ok(());
        }

        // Try launchctl on macOS
        #[cfg(target_os = "macos")]
        {
            let plist_path = dirs::home_dir()
                .unwrap()
                .join("Library/LaunchAgents/com.treerag.daemon.plist");

            if plist_path.exists() {
                let status = std::process::Command::new("launchctl")
                    .args(["load", "-w"])
                    .arg(&plist_path)
                    .status()?;

                if status.success() {
                    println!("✓ TreeRAG daemon started via launchctl");
                    return Ok(());
                }
            }
        }

        // Fallback: start in background
        let child = std::process::Command::new("treerag-daemon")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to start daemon")?;

        println!("✓ TreeRAG daemon started (PID: {})", child.id());
    }

    Ok(())
}

async fn cmd_stop() -> Result<()> {
    let client = IpcClient::new();

    if !client.is_daemon_running() {
        println!("TreeRAG daemon is not running.");
        return Ok(());
    }

    match client.request(Request::Shutdown).await {
        Ok(Response::Ack) => {
            println!("✓ TreeRAG daemon stopping...");

            // Wait a moment for cleanup
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            if !client.is_daemon_running() {
                println!("✓ Daemon stopped.");
            }
        }
        Ok(resp) => {
            println!("Unexpected response: {:?}", resp);
        }
        Err(e) => {
            println!("Failed to stop daemon: {}", e);
        }
    }

    Ok(())
}

async fn cmd_status() -> Result<()> {
    let client = IpcClient::new();

    if !client.is_daemon_running() {
        println!("TreeRAG daemon is not running.");
        println!("\nStart with: treerag start");
        return Ok(());
    }

    match client.get_status().await {
        Ok(ResponseData::Status {
            version,
            uptime_secs,
            projects_loaded,
            memory_usage_bytes,
            requests_total,
            cache_hit_rate,
            avg_latency_ms,
        }) => {
            println!("TreeRAG Daemon v{}", version);
            println!();
            println!("  Status:     Running");
            println!("  Uptime:     {}", format_duration(uptime_secs));
            println!("  Projects:   {} loaded", projects_loaded);
            println!(
                "  Memory:     {:.1} MB",
                memory_usage_bytes as f64 / 1024.0 / 1024.0
            );
            println!();
            println!("  Requests:   {}", requests_total);
            println!("  Cache Hit:  {:.1}%", cache_hit_rate * 100.0);
            println!("  Avg Latency: {}ms", avg_latency_ms);
        }
        Ok(_) => {
            println!("Unexpected status response");
        }
        Err(e) => {
            println!("Failed to get status: {}", e);
        }
    }

    Ok(())
}

async fn cmd_init(path: &str, quick: bool) -> Result<()> {
    let cwd = PathBuf::from(path).canonicalize().context("Invalid path")?;

    println!("Initializing TreeRAG for: {}", cwd.display());

    let client = IpcClient::new();

    if !client.is_daemon_running() {
        println!("✗ Daemon not running. Start with: treerag start");
        return Ok(());
    }

    // Check if already initialized
    match client.is_project_initialized(&cwd).await {
        Ok(true) => {
            println!("✓ Project is already initialized.");
            return Ok(());
        }
        Ok(false) => {}
        Err(e) => {
            println!("✗ Failed to check project: {}", e);
            return Ok(());
        }
    }

    // Initialize project
    let request = Request::InitProject {
        cwd: cwd.clone(),
        async_mode: !quick,
    };

    match client.request(request).await {
        Ok(Response::Ok { .. }) => {
            println!("✓ Project initialized successfully!");

            if !quick {
                println!();
                println!("AI enrichment is running in the background.");
                println!("Check status with: treerag project");
            }
        }
        Ok(Response::Error { message, .. }) => {
            println!("✗ Initialization failed: {}", message);
        }
        Ok(_) => {
            println!("✗ Unexpected response");
        }
        Err(e) => {
            println!("✗ Error: {}", e);
        }
    }

    Ok(())
}

async fn cmd_project(path: &str) -> Result<()> {
    let cwd = PathBuf::from(path).canonicalize().context("Invalid path")?;

    let client = IpcClient::new();

    if !client.is_daemon_running() {
        println!("TreeRAG daemon is not running.");
        return Ok(());
    }

    match client.is_project_initialized(&cwd).await {
        Ok(true) => {
            println!("Project: {}", cwd.display());
            println!("  Status: Initialized");
            // TODO: Load more project info (file count, languages, etc.)
        }
        Ok(false) => {
            println!("Project: {}", cwd.display());
            println!("  Status: Not initialized");
            println!();
            println!("Initialize with: treerag init");
        }
        Err(e) => {
            println!("Failed to check project: {}", e);
        }
    }

    Ok(())
}

async fn cmd_ping() -> Result<()> {
    let client = IpcClient::new();

    if !client.is_daemon_running() {
        println!("✗ Daemon not running");
        return Ok(());
    }

    let start = std::time::Instant::now();
    match client.request(Request::Ping).await {
        Ok(Response::Ok {
            data: Some(ResponseData::Pong { .. }),
        }) => {
            let elapsed = start.elapsed();
            println!("✓ Pong! ({:.2}ms)", elapsed.as_secs_f64() * 1000.0);
        }
        Ok(_) => {
            println!("✗ Unexpected response");
        }
        Err(e) => {
            println!("✗ Error: {}", e);
        }
    }

    Ok(())
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}
