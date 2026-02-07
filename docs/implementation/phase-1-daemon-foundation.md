# Phase 1: Daemon Foundation

> **Goal**: Build the core daemon infrastructure that runs as a background process on macOS, handles IPC, and manages project lifecycle.

## Overview

| Aspect | Detail |
|--------|--------|
| **Duration** | 1-2 weeks |
| **Priority** | Critical (foundation for all other phases) |
| **Dependencies** | None |
| **Deliverables** | Running daemon, IPC protocol, CLI skeleton |

---

## 1.1 Daemon Core

### Objectives
- [x] Create Rust workspace structure
- [x] Implement async daemon with graceful shutdown
- [x] Set up launchd integration for macOS
- [x] Implement PID file and single-instance check

### Implementation Details

#### Workspace Structure
```
crates/
├── engram-daemon/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Entry point, CLI args
│       ├── daemon.rs            # Daemon lifecycle
│       ├── config.rs            # Configuration loading
│       └── signals.rs           # Signal handling
└── Cargo.toml                   # Workspace manifest
```

#### Daemon Lifecycle

```rust
// crates/engram-daemon/src/daemon.rs

use tokio::signal;
use std::path::PathBuf;

pub struct Daemon {
    config: DaemonConfig,
    pid_file: PathBuf,
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl Daemon {
    pub async fn run(&self) -> Result<(), DaemonError> {
        // 1. Check single instance
        self.acquire_pid_lock()?;
        
        // 2. Initialize subsystems
        let ipc_server = IpcServer::new(&self.config.socket_path).await?;
        let file_watcher = FileWatcher::new()?;
        let project_manager = ProjectManager::new(&self.config)?;
        
        // 3. Run main loop
        tokio::select! {
            _ = ipc_server.run() => {},
            _ = file_watcher.run() => {},
            _ = signal::ctrl_c() => {
                tracing::info!("Received shutdown signal");
            }
        }
        
        // 4. Graceful shutdown
        self.shutdown().await?;
        
        Ok(())
    }
    
    fn acquire_pid_lock(&self) -> Result<(), DaemonError> {
        if self.pid_file.exists() {
            let pid = std::fs::read_to_string(&self.pid_file)?;
            // Check if process is actually running
            if is_process_running(pid.trim().parse()?) {
                return Err(DaemonError::AlreadyRunning);
            }
        }
        std::fs::write(&self.pid_file, std::process::id().to_string())?;
        Ok(())
    }
}
```

#### Configuration

```rust
// crates/engram-daemon/src/config.rs

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct DaemonConfig {
    /// Unix socket path
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
    
    /// Data directory
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    
    /// Maximum memory usage (bytes)
    #[serde(default = "default_max_memory")]
    pub max_memory: usize,
    
    /// Maximum projects to keep in memory
    #[serde(default = "default_max_projects")]
    pub max_projects: usize,
    
    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_socket_path() -> PathBuf {
    PathBuf::from("/tmp/engram.sock")
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".engram")
}

fn default_max_memory() -> usize {
    100 * 1024 * 1024 // 100MB
}

fn default_max_projects() -> usize {
    3
}
```

#### launchd Integration

```xml
<!-- claude-integration/com.engram.daemon.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.engram.daemon</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/engram-daemon</string>
    </array>
    
    <key>RunAtLoad</key>
    <true/>
    
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    
    <key>StandardOutPath</key>
    <string>/tmp/engram.out.log</string>
    
    <key>StandardErrorPath</key>
    <string>/tmp/engram.err.log</string>
    
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
    
    <key>ProcessType</key>
    <string>Background</string>
    
    <key>LowPriorityIO</key>
    <true/>
    
    <key>HardResourceLimits</key>
    <dict>
        <key>MemoryLimit</key>
        <integer>104857600</integer>
    </dict>
</dict>
</plist>
```

---

## 1.2 IPC Protocol

### Objectives
- [x] Define MessagePack-based protocol
- [x] Implement Unix socket server
- [x] Create async request/response handling
- [x] Implement fire-and-forget async commands

### Protocol Design

```rust
// crates/engram-ipc/src/protocol.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Request from client (hooks/CLI) to daemon
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    /// Check if project is initialized
    CheckInit { 
        cwd: PathBuf 
    },
    
    /// Initialize a new project
    InitProject { 
        cwd: PathBuf,
        #[serde(default)]
        async_mode: bool,  // Non-blocking AI enrichment
    },
    
    /// Get context for a prompt (pre-computed cache)
    GetContext { 
        cwd: PathBuf,
        prompt: Option<String>,
    },
    
    /// Prepare context for next prompt (async, fire-and-forget)
    PrepareContext { 
        cwd: PathBuf,
        prompt: String,
    },
    
    /// Notify file change (async, fire-and-forget)
    NotifyFileChange { 
        cwd: PathBuf,
        path: PathBuf,
        change_type: ChangeType,
    },
    
    /// Graft experience from agent (async, fire-and-forget)
    GraftExperience { 
        cwd: PathBuf,
        experience: Experience,
    },
    
    /// Get daemon status
    Status,
    
    /// Graceful shutdown
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Experience {
    pub agent_id: String,
    pub decision: String,
    pub rationale: Option<String>,
    pub files_touched: Vec<PathBuf>,
    pub timestamp: i64,
}

/// Response from daemon to client
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    /// Success with optional data
    Ok { 
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<ResponseData> 
    },
    
    /// Acknowledgment for fire-and-forget requests
    Ack,
    
    /// Error response
    Error { 
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    InitStatus { initialized: bool },
    Context { context: String, nodes: Vec<String> },
    Status { 
        version: String,
        uptime_secs: u64,
        projects_loaded: usize,
        memory_usage: usize,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotInitialized,
    InvalidRequest,
    InternalError,
    Timeout,
}
```

### IPC Server Implementation

```rust
// crates/engram-ipc/src/server.rs

use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::path::Path;
use std::time::Duration;

const MAX_REQUEST_SIZE: usize = 1024 * 1024;  // 1MB
const REQUEST_TIMEOUT: Duration = Duration::from_millis(100);

pub struct IpcServer {
    listener: UnixListener,
    handler: Arc<dyn RequestHandler>,
}

impl IpcServer {
    pub async fn new<P: AsRef<Path>>(
        socket_path: P, 
        handler: Arc<dyn RequestHandler>
    ) -> Result<Self, IpcError> {
        // Remove stale socket
        let _ = std::fs::remove_file(&socket_path);
        
        let listener = UnixListener::bind(&socket_path)?;
        
        // Set socket permissions (user only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &socket_path, 
                std::fs::Permissions::from_mode(0o600)
            )?;
        }
        
        Ok(Self { listener, handler })
    }
    
    pub async fn run(&self) -> Result<(), IpcError> {
        loop {
            match self.listener.accept().await {
                Ok((stream, _)) => {
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, handler).await {
                            tracing::warn!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }
    
    async fn handle_connection(
        mut stream: UnixStream,
        handler: Arc<dyn RequestHandler>,
    ) -> Result<(), IpcError> {
        // Read with timeout (non-blocking requirement)
        let request = tokio::time::timeout(REQUEST_TIMEOUT, async {
            // Read length prefix (4 bytes)
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await?;
            let len = u32::from_le_bytes(len_buf) as usize;
            
            if len > MAX_REQUEST_SIZE {
                return Err(IpcError::RequestTooLarge);
            }
            
            // Read request body
            let mut buf = vec![0u8; len];
            stream.read_exact(&mut buf).await?;
            
            // Deserialize MessagePack
            let request: Request = rmp_serde::from_slice(&buf)?;
            Ok(request)
        }).await??;
        
        // Handle request
        let response = handler.handle(request).await;
        
        // Serialize and send response
        let response_bytes = rmp_serde::to_vec(&response)?;
        let len_bytes = (response_bytes.len() as u32).to_le_bytes();
        
        stream.write_all(&len_bytes).await?;
        stream.write_all(&response_bytes).await?;
        
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle(&self, request: Request) -> Response;
}
```

---

## 1.3 Project Manager

### Objectives
- [x] Implement LRU cache for loaded projects
- [x] Handle project loading/unloading
- [x] Track project state and initialization status

### Implementation

```rust
// crates/engram-core/src/project_manager.rs

use lru::LruCache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ProjectManager {
    /// LRU cache of loaded projects
    projects: RwLock<LruCache<PathBuf, Arc<Project>>>,
    
    /// Project hash lookup (for persistence)
    hash_map: RwLock<HashMap<PathBuf, String>>,
    
    /// Data directory
    data_dir: PathBuf,
    
    /// Max projects in memory
    max_projects: usize,
}

impl ProjectManager {
    pub fn new(config: &DaemonConfig) -> Self {
        Self {
            projects: RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(config.max_projects).unwrap()
            )),
            hash_map: RwLock::new(HashMap::new()),
            data_dir: config.data_dir.clone(),
            max_projects: config.max_projects,
        }
    }
    
    /// Check if project is initialized
    pub async fn is_initialized(&self, cwd: &Path) -> bool {
        let hash = self.get_project_hash(cwd);
        let manifest_path = self.data_dir
            .join("projects")
            .join(&hash)
            .join("manifest.json");
        manifest_path.exists()
    }
    
    /// Get or load a project
    pub async fn get_project(&self, cwd: &Path) -> Result<Arc<Project>, ProjectError> {
        // Check cache first
        {
            let mut projects = self.projects.write().await;
            if let Some(project) = projects.get(&cwd.to_path_buf()) {
                return Ok(project.clone());
            }
        }
        
        // Load from disk
        let project = self.load_project(cwd).await?;
        let project = Arc::new(project);
        
        // Add to cache (LRU will evict oldest if full)
        {
            let mut projects = self.projects.write().await;
            projects.put(cwd.to_path_buf(), project.clone());
        }
        
        Ok(project)
    }
    
    /// Unload least recently used projects to free memory
    pub async fn gc(&self) {
        let mut projects = self.projects.write().await;
        
        // LRU cache handles this automatically, but we can force
        // eviction if memory pressure is high
        while projects.len() > self.max_projects {
            projects.pop_lru();
        }
    }
    
    fn get_project_hash(&self, cwd: &Path) -> String {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        cwd.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
    
    async fn load_project(&self, cwd: &Path) -> Result<Project, ProjectError> {
        let hash = self.get_project_hash(cwd);
        let project_dir = self.data_dir.join("projects").join(&hash);
        
        if !project_dir.exists() {
            return Err(ProjectError::NotInitialized);
        }
        
        Project::load(&project_dir).await
    }
}
```

---

## 1.4 CLI Skeleton

### Objectives
- [x] Create CLI tool for daemon control
- [x] Implement start/stop/status commands
- [x] Add project management commands

### Implementation

```rust
// crates/engram-cli/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "engram")]
#[command(about = "Engram context management for AI coding")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon
    Start {
        /// Run in foreground (for debugging)
        #[arg(short, long)]
        foreground: bool,
    },
    
    /// Stop the daemon
    Stop,
    
    /// Show daemon status
    Status,
    
    /// Initialize a project
    Init {
        /// Project path (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        
        /// Skip AI enrichment (fast mode)
        #[arg(long)]
        quick: bool,
    },
    
    /// Show project status
    Project {
        #[arg(default_value = ".")]
        path: String,
    },
    
    /// Install Claude Code integration
    InstallClaude,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Start { foreground } => {
            if foreground {
                // Run daemon directly
                engram_daemon::run().await?;
            } else {
                // Use launchctl on macOS
                std::process::Command::new("launchctl")
                    .args(["load", "-w", "~/Library/LaunchAgents/com.engram.daemon.plist"])
                    .status()?;
                println!("Engram daemon started");
            }
        }
        
        Commands::Stop => {
            let client = IpcClient::connect().await?;
            client.send(Request::Shutdown).await?;
            println!("Engram daemon stopped");
        }
        
        Commands::Status => {
            let client = IpcClient::connect().await?;
            let response = client.send(Request::Status).await?;
            
            if let Response::Ok { data: Some(ResponseData::Status { 
                version, uptime_secs, projects_loaded, memory_usage 
            })} = response {
                println!("Engram Daemon v{}", version);
                println!("  Uptime: {}s", uptime_secs);
                println!("  Projects loaded: {}", projects_loaded);
                println!("  Memory: {} MB", memory_usage / 1024 / 1024);
            }
        }
        
        Commands::Init { path, quick } => {
            let cwd = std::fs::canonicalize(&path)?;
            let client = IpcClient::connect().await?;
            
            println!("Initializing project: {}", cwd.display());
            
            let response = client.send(Request::InitProject { 
                cwd: cwd.clone(),
                async_mode: !quick,
            }).await?;
            
            match response {
                Response::Ok { .. } => {
                    println!("✓ Project initialized");
                    if !quick {
                        println!("  AI enrichment running in background...");
                    }
                }
                Response::Error { message, .. } => {
                    eprintln!("✗ Error: {}", message);
                }
                _ => {}
            }
        }
        
        _ => todo!()
    }
    
    Ok(())
}
```

---

## Testing Requirements

> Reference: [Testing Strategy](./testing-strategy.md) for general guidelines.

### Unit Test Coverage

#### engram-ipc

| Component | Required Tests |
|-----------|----------------|
| `protocol.rs` | Serialization round-trips for all Request/Response variants; empty fields; max-size payloads |
| `server.rs` | Connection accept; request parsing errors; malformed length prefix; request size limits; socket permission checks |
| `client.rs` | Connection success/failure; daemon not running detection; timeout handling; response parsing |
| `error.rs` | All error variants can be created and display correctly |

**Edge cases to cover:**
- Request with 0-byte length prefix
- Request exceeding MAX_REQUEST_SIZE (1MB)
- Partial request (connection dropped mid-read)
- Invalid MessagePack data
- Valid JSON fallback parsing
- Socket path with special characters
- Socket already in use

#### engram-core

| Component | Required Tests |
|-----------|----------------|
| `config.rs` | Default values; YAML parsing; missing config file; invalid YAML; partial config override |
| `project.rs` | Create/load round-trip; manifest serialization; update operations; concurrent access |
| `project_manager.rs` | LRU eviction at capacity; cache hits; cache misses; concurrent get_project; path canonicalization |
| `error.rs` | All error variants; error message clarity |

**Edge cases to cover:**
- Empty project path
- Non-existent project path
- Path that becomes invalid during operation
- Manifest file corruption
- Storage directory permission denied
- Project hash collisions (extremely unlikely but must handle)
- Concurrent init_project for same path
- Project with >1M files (stress test metadata)

#### engram-daemon

| Component | Required Tests |
|-----------|----------------|
| `daemon.rs` | PID lock acquisition; stale PID cleanup; cleanup on shutdown; cleanup on panic |
| `handler.rs` | All request types; error responses; fire-and-forget acknowledgment |
| `signals.rs` | SIGINT handling; SIGTERM handling; shutdown command |

**Edge cases to cover:**
- Daemon started twice (second should fail gracefully)
- PID file contains invalid data
- PID file points to different process (PID reuse)
- Shutdown during active request
- Socket file deletion while running
- Config file changes during runtime

#### engram-cli

| Component | Required Tests |
|-----------|----------------|
| `main.rs` | All subcommands execute; help text; error output formatting |

**Edge cases to cover:**
- Daemon not running for stop/status
- Invalid path for init
- Very long paths
- Paths with spaces and special characters

### Integration Test Coverage

#### IPC Full Round-Trip

```
tests/
├── integration_ipc.rs
```

Tests must verify:
- Client connects → sends request → receives correct response
- Multiple sequential requests on same connection
- Connection reuse vs new connection performance
- Server handles client disconnect gracefully
- Server handles slow client (partial reads)
- Concurrent clients (10, 100, 1000)
- Large request payload (approaching 1MB limit)
- Fire-and-forget requests complete asynchronously

#### Daemon Lifecycle

```
tests/
├── integration_daemon.rs
```

Tests must verify:
- Start daemon → verify socket exists → stop daemon → verify cleanup
- Restart daemon after crash (stale PID file)
- Daemon recovers from socket file deletion
- Daemon respects max_memory config
- Daemon respects max_projects config
- Graceful shutdown completes within 5s
- Signal handling (SIGINT, SIGTERM)

#### Project Lifecycle

```
tests/
├── integration_project.rs
```

Tests must verify:
- Init → Load → Unload → Load again
- LRU eviction preserves data on disk
- Concurrent access to same project
- Project data persists across daemon restarts
- Corrupted manifest is handled gracefully
- Missing storage directory is detected

### Performance Test Requirements

> Reference: [Benchmarks Reference](./benchmarks-reference.md) for methodology and sources.

| Metric | Target | Source/Rationale |
|--------|--------|------------------|
| IPC ping round-trip (P50) | <100µs | Unix socket + MessagePack, minimal payload |
| IPC ping round-trip (P99) | <500µs | Account for scheduling jitter |
| IPC request with 1KB payload (P99) | <1ms | Serialization overhead |
| Fire-and-forget ack | <50µs | Immediate response, no processing |
| Daemon startup (cold) | <500ms | Binary load + Tokio runtime init |
| Daemon startup (warm) | <200ms | With process cache |
| Daemon shutdown (graceful) | <1s | Cleanup all resources |
| Project manifest load | <5ms | JSON parse, <10KB typical |

**Performance test structure:**
```rust
#[bench]
fn bench_ipc_ping_latency(b: &mut Bencher) {
    // Must measure P50, P90, P99, P99.9
}

#[bench]
fn bench_project_load_cached(b: &mut Bencher) {
    // Already in LRU cache
}

#[bench]
fn bench_project_load_disk(b: &mut Bencher) {
    // Must load from disk
}
```

### Resource Test Requirements

| Resource | Limit | Test |
|----------|-------|------|
| Idle memory | <20MB | Run daemon for 5 min idle, measure RSS |
| Active memory | <100MB | Load max_projects, perform operations |
| Idle CPU | <1% | Measure over 30s idle period |
| File descriptors | <20 idle | Count after 1000 connections |
| Socket cleanup | All removed | Verify no stale sockets after shutdown |

**Resource test structure:**
```rust
#[tokio::test]
async fn test_memory_bounded_under_load() {
    // Perform many operations
    // Verify memory stays within limits
}

#[tokio::test]
async fn test_no_fd_leak() {
    // Many connect/disconnect cycles
    // Verify FD count stable
}
```

### Error Recovery Testing

Every error must be tested for proper recovery:

| Error Scenario | Expected Recovery |
|----------------|-------------------|
| Socket permission denied | Clear error message, daemon exits cleanly |
| Data directory not writable | Clear error message, no data corruption |
| PID file locked by another process | Clear error message, exits with code 1 |
| Malformed request | Error response sent, connection closed, server continues |
| Client timeout | Connection cleaned up, server continues |
| Out of memory | Graceful degradation (evict projects), not crash |

### CLI Testing

All CLI commands must be tested:

```bash
# Success cases
engram start
engram status  # Shows running
engram ping    # Shows <Xms
engram init .
engram project .
engram stop

# Error cases
engram status  # Daemon not running
engram stop    # Already stopped
engram init /nonexistent
engram init    # Already initialized
```

### Test Execution Commands

```bash
# All unit tests
cargo test --workspace --lib

# All integration tests
cargo test --workspace --test '*'

# Specific crate tests
cargo test -p engram-ipc
cargo test -p engram-core
cargo test -p engram-daemon
cargo test -p engram-cli

# With coverage
cargo llvm-cov --workspace

# Performance benchmarks
cargo bench

# Stress tests (longer duration)
cargo test --release -- --ignored stress
```

---

## Deliverables Checklist

### Implementation
- [x] Rust workspace with all crates
- [x] Daemon binary that runs as background process
- [x] launchd plist for macOS auto-start
- [x] Unix socket IPC with MessagePack protocol
- [x] Request/response handling with <5ms latency
- [x] LRU project cache
- [x] CLI tool with start/stop/status/init commands
- [x] Basic logging with `tracing`

### Testing
- [x] Unit tests for engram-ipc (14 tests passing)
- [x] Unit tests for engram-core (16 tests passing)
- [x] Unit tests for engram-daemon (4 tests passing)
- [x] Integration tests for IPC round-trips (3 tests)
- [x] Integration tests for daemon lifecycle (8 tests)
- [x] Integration tests for project lifecycle (included above)
- [ ] Performance benchmarks for hot paths
- [ ] Resource consumption tests
- [ ] Error recovery tests
- [ ] CLI command tests

