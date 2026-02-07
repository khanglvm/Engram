# Engram Implementation Plan

> **Vectorless, Reasoning-Based Context Management for AI Coding Agents**

## Project Overview

Engram is a daemon-based context management system that provides intelligent, structured context to AI coding assistants (Claude Code, OpenCode, Gemini CLI) for large codebases. It replaces traditional file-based instruction systems with hierarchical tree indexing and hybrid retrieval.

### Core Value Proposition

| Problem | Engram Solution |
|---------|------------------|
| Context rot in long sessions | Dynamic tree slicing with focused context |
| Slow knowledge retrieval | Hybrid vector + tree search, pre-computed cache |
| Lost context between sessions | Persistent memory with experience pooling |
| Sub-agent context isolation | Elastic focus with peripheral vision |
| Manual context management | Automatic indexing with daemon-based sync |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Engram SYSTEM                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    ENGRAM DAEMON (Single Process)                  │   │
│  │                                                                      │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────────────┐    │   │
│  │  │ IPC Server  │  │ File Watcher│  │   Project Manager        │    │   │
│  │  │ (Unix Sock) │  │ (FSEvents)  │  │   (Lazy Load/Unload)     │    │   │
│  │  └─────────────┘  └─────────────┘  └──────────────────────────┘    │   │
│  │                                                                      │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────────────┐    │   │
│  │  │ Tree Index  │  │ Vector Index│  │   Context Manager        │    │   │
│  │  │ (PageIndex) │  │ (Hybrid)    │  │   (Sandwich Builder)     │    │   │
│  │  └─────────────┘  └─────────────┘  └──────────────────────────┘    │   │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                          ▲                                                  │
│                          │ Unix Socket + MessagePack                       │
│                          ▼                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │                    CLAUDE CODE INTEGRATION                            │ │
│  │  Hooks: SessionStart, UserPrompt, SubagentStart/Stop, PreCompact     │ │
│  │  Commands: /init-project, /scout-status, /refresh-context            │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

| Phase | Focus | Duration | Dependencies |
|-------|-------|----------|--------------|
| [Phase 1](./phase-1-daemon-foundation.md) | Core Daemon & IPC | 1-2 weeks | None |
| [Phase 2](./phase-2-indexing-storage.md) | Indexing & Persistence | 2-3 weeks | Phase 1 |
| [Phase 3](./phase-3-context-management.md) | Context Manager & Hybrid Retrieval | 2 weeks | Phase 2 |
| [Phase 4](./phase-4-claude-integration.md) | Claude Code Integration | 1-2 weeks | Phase 3 |
| [Phase 5](./phase-5-optimization.md) | Optimization & Polish | 1-2 weeks | Phase 4 |

---

## Technology Stack

### Core Daemon (Rust)
- **Why Rust**: Memory safety, zero-cost abstractions, excellent async runtime
- **Key crates**:
  - `tokio` - Async runtime
  - `tokio-unix` - Unix socket IPC
  - `rmp-serde` - MessagePack serialization
  - `notify` - Cross-platform file watching (FSEvents on macOS)
  - `memmap2` - Memory-mapped files
  - `lru` - LRU cache implementation
  - `tree-sitter` - Fast AST parsing

### Indexing
- **Tree Index**: Custom implementation inspired by PageIndex
- **Vector Index**: `usearch` (lightweight, fast) or `faiss` (more features)
- **Embeddings**: `fastembed-rs` for local embeddings (no API calls)

### Storage
- **Format**: MessagePack for binary, JSON for human-readable
- **Strategy**: Memory-mapped files for large trees

### Claude Code Integration
- **Hooks**: Shell scripts calling Unix socket
- **Commands**: Markdown files in `.claude/commands/`

---

## Performance Targets

| Metric | Target | Critical Path |
|--------|--------|---------------|
| Hook latency | <5ms | Unix socket + cache lookup |
| Daemon memory | <100MB | LRU + mmap + lazy loading |
| CPU (idle) | <1% | Event-driven, no polling |
| Context injection | <3ms | Pre-computed cache |
| Project init (algorithmic) | <30s | AST parsing |
| Project init (AI enrichment) | <5min | Background, parallel |

---

## Directory Structure

```
Engram/
├── docs/
│   └── implementation/          # This directory
│       ├── README.md            # Overview (this file)
│       ├── phase-1-*.md         # Phase plans
│       └── ...
├── crates/                      # Rust workspace
│   ├── engram-daemon/          # Main daemon binary
│   ├── engram-core/            # Core library
│   ├── engram-indexer/         # Indexing logic
│   ├── engram-ipc/             # IPC protocol
│   └── engram-cli/             # CLI tool
├── claude-integration/          # Claude Code plugin
│   ├── hooks/                   # Hook scripts
│   ├── commands/                # Slash commands
│   └── install.sh               # Installer
├── tests/                       # Integration tests
└── Cargo.toml                   # Workspace manifest
```

---

## Quick Links

- [Phase 1: Daemon Foundation](./phase-1-daemon-foundation.md)
- [Phase 2: Indexing & Storage](./phase-2-indexing-storage.md)
- [Phase 3: Context Management](./phase-3-context-management.md)
- [Phase 4: Claude Code Integration](./phase-4-claude-integration.md)
- [Phase 5: Optimization & Polish](./phase-5-optimization.md)

---

## Development Workflow

```bash
# Build all crates
cargo build --release

# Run daemon in development
cargo run -p engram-daemon -- --dev

# Run tests
cargo test --workspace

# Install Claude Code integration
./claude-integration/install.sh

# Check daemon status
engram status
```
