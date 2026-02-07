# TreeRAG

> **Intelligent Context Management for AI Coding Agents**

TreeRAG provides smart, focused context to AI coding assistants by maintaining a daemon that understands your codebase structure, dependencies, and history.

## Features

- **Fast Indexing**: Scans codebases in seconds using tree-sitter
- **Smart Context**: Hybrid retrieval balances structure and semantics
- **Low Overhead**: <100MB memory, <1% CPU when idle
- **Claude Integration**: Seamless hooks for Claude Code

## Quick Start

### 1. Build

```bash
cd /path/to/TreeRAG
cargo build --release
```

### 2. Start the Daemon

```bash
cargo run --release --bin treerag-daemon
```

### 3. Initialize a Project

```bash
cd /your/project
cargo run --release --bin treerag-cli -- init .
```

### 4. Install Claude Integration (optional)

```bash
./claude-integration/install.sh
```

Now open Claude Code in your project - context is injected automatically!

## Project Structure

```
TreeRAG/
├── crates/
│   ├── treerag-cli/       # Command-line interface
│   ├── treerag-core/      # Project management, config, metrics
│   ├── treerag-context/   # Context manager, hybrid router
│   ├── treerag-daemon/    # Background daemon process
│   ├── treerag-indexer/   # Scanner, parser, storage
│   └── treerag-ipc/       # Client/server IPC protocol
├── claude-integration/    # Claude Code hooks and commands
├── docs/                  # Documentation
│   └── implementation/    # Phase implementation plans
└── integration/           # launchd plist for auto-start
```

## Commands

| Command | Description |
|---------|-------------|
| `treerag start` | Start the daemon |
| `treerag stop` | Stop the daemon |
| `treerag status` | Show daemon status |
| `treerag init <path>` | Initialize project indexing |
| `treerag project <path>` | Show project info |
| `treerag ping` | Check daemon responsiveness |

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
