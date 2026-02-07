# Engram

> **Intelligent Context Management for AI Coding Agents**

Engram provides smart, focused context to AI coding assistants by maintaining a daemon that understands your codebase structure, dependencies, and history.

## Features

- **Fast Indexing**: Scans codebases in seconds using tree-sitter
- **Smart Context**: Hybrid retrieval balances structure and semantics
- **Low Overhead**: <100MB memory, <1% CPU when idle
- **Claude Integration**: Seamless hooks for Claude Code

## Quick Start

### 1. Build

```bash
cd /path/to/Engram
cargo build --release
```

### 2. Start the Daemon

```bash
cargo run --release --bin engram-daemon
```

### 3. Initialize a Project

```bash
cd /your/project
cargo run --release --bin engram-cli -- init .
```

### 4. Install Claude Integration (optional)

```bash
./claude-integration/install.sh
```

Now open Claude Code in your project - context is injected automatically!

## Project Structure

```
Engram/
├── crates/
│   ├── engram-cli/       # Command-line interface
│   ├── engram-core/      # Project management, config, metrics
│   ├── engram-context/   # Context manager, hybrid router
│   ├── engram-daemon/    # Background daemon process
│   ├── engram-indexer/   # Scanner, parser, storage
│   └── engram-ipc/       # Client/server IPC protocol
├── claude-integration/    # Claude Code hooks and commands
├── docs/                  # Documentation
│   └── implementation/    # Phase implementation plans
└── integration/           # launchd plist for auto-start
```

## Commands

| Command | Description |
|---------|-------------|
| `engram start` | Start the daemon |
| `engram stop` | Stop the daemon |
| `engram status` | Show daemon status |
| `engram init <path>` | Initialize project indexing |
| `engram project <path>` | Show project info |
| `engram ping` | Check daemon responsiveness |

## Claude Slash Commands

| Command | Description |
|---------|-------------|
| `/init-project` | Initialize current project |
| `/scout-status` | Show indexing status |
| `/refresh-context` | Force context refresh |

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for system design.

## License

MIT
