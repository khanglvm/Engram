# Engram

> Daemon-powered memory and context engine for AI coding assistants

Engram is a daemon-based system that provides intelligent, structured context to AI coding assistants like Claude Code. It uses hierarchical tree indexing and hybrid retrieval to help AI agents navigate large codebases efficiently.

Current binary and crate names still use the `treerag` prefix.

## Features

- **🔥 Non-blocking hooks**: All Claude Code integrations complete in <5ms
- **🧠 Smart context**: Automatic dependency loading and context prioritization
- **💾 Persistent memory**: Agent decisions persist across sessions
- **📊 Hybrid retrieval**: Combines tree-based and vector search
- **🔄 Real-time updates**: Incremental re-indexing on file changes
- **💡 Low resource usage**: <100MB memory, <1% CPU when idle

## Installation

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- macOS (for launchd integration)

### Build from source

```bash
# Clone the repository
git clone git@github.com:khanglvm/Engram.git
cd Engram

# Build release binaries
cargo build --release

# Install binaries
cargo install --path crates/treerag-cli
cargo install --path crates/treerag-daemon

# Install launchd service (optional, for auto-start)
cp integration/com.treerag.daemon.plist ~/Library/LaunchAgents/
```

## Quick Start

```bash
# Start the daemon
treerag start

# Check status
treerag status

# Initialize a project
cd /path/to/your/project
treerag init

# The daemon is now tracking your project!
```

## Claude Code Integration

Engram integrates with Claude Code via hooks that inject relevant context automatically:

```bash
# Install Claude Code integration
treerag install-claude
```

Or use the `/init-project` slash command directly in Claude Code.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                         Claude Code                          │
│                                                              │
│   Hooks:  SessionStart → UserPromptSubmit → PostToolUse     │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼ Unix Socket (MessagePack)
┌──────────────────────────────────────────────────────────────┐
│                       Engram Daemon                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ IPC Server  │  │   Project   │  │  Context Manager    │  │
│  │             │  │   Manager   │  │  (Hybrid Retrieval) │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│                          │                    │              │
│                          ▼                    ▼              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                   Storage Layer                         ││
│  │  • Memory-mapped tree files                             ││
│  │  • Experience log (JSONL)                               ││
│  │  • Project manifests                                    ││
│  └─────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `treerag start` | Start the daemon |
| `treerag stop` | Stop the daemon |
| `treerag status` | Show daemon status |
| `treerag init [path]` | Initialize a project |
| `treerag project [path]` | Show project info |
| `treerag ping` | Check daemon connectivity |

## Development

```bash
# Run tests
cargo test --workspace

# Run daemon in foreground (for development)
RUST_LOG=debug treerag start --foreground

# Check IPC connectivity
echo '{"action":"ping"}' | nc -U /tmp/treerag.sock
```

## Configuration

Configuration is stored in `~/.treerag/config.yaml`:

```yaml
# Socket path for IPC
socket_path: /tmp/treerag.sock

# Data directory for project storage
data_dir: ~/.treerag

# Maximum memory usage (bytes)
max_memory: 104857600  # 100MB

# Maximum projects in LRU cache
max_projects: 3

# Log level
log_level: info
```

## Project Data

Project data is stored in `~/.treerag/projects/<hash>/`:

```
~/.treerag/projects/<hash>/
├── manifest.json      # Project metadata
├── tree.mmap          # Memory-mapped tree structure
├── experience.jsonl   # Agent decision log
└── snapshots/         # Point-in-time snapshots
```

## Performance Targets

| Metric | Target |
|--------|--------|
| Hook latency | <5ms (P99) |
| Memory usage | <100MB |
| CPU (idle) | <1% |
| Project scan | <30s for 10k files |

## License

MIT
