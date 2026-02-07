# TreeRAG Architecture

## System Overview

TreeRAG is a daemon-based context management system that provides AI coding agents with smart, focused context from large codebases.

```
┌─────────────────────────────────────────────────────────────┐
│                     Claude Code                              │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        │
│  │ Session │  │ Prompt  │  │  Tool   │  │Subagent │        │
│  │  Hook   │  │  Hook   │  │  Hook   │  │  Hook   │        │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘        │
└───────┼────────────┼────────────┼────────────┼──────────────┘
        │            │            │            │
        ▼            ▼            ▼            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Unix Socket IPC                            │
└─────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│                   TreeRAG Daemon                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │   Project    │  │   Context    │  │   Metrics    │      │
│  │   Manager    │  │   Manager    │  │   Monitor    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                            │                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │    Tree      │  │  Experience  │  │    File      │      │
│  │   Storage    │  │     Pool     │  │   Watcher    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

## Crate Responsibilities

### treerag-ipc
- MessagePack serialization over Unix sockets
- Request/Response protocol
- Async client and server

### treerag-core
- Project management with LRU cache
- Configuration loading (YAML)
- Metrics and monitoring

### treerag-indexer
- Fast file scanner with gitignore support
- Tree-sitter parsing (Rust, Python, TypeScript, JavaScript)
- Tree data structure with dependencies
- Persistent storage (JSON + MessagePack)
- File watcher with debouncing

### treerag-context
- Context Manager with scopes
- Hybrid router (tree-based retrieval)
- Context Sandwich builder (layers)
- Experience pool for agent decisions

### treerag-daemon
- Background process lifecycle
- Request handler routing
- Integration with all crates

### treerag-cli
- User-facing commands
- Daemon control

## Data Flow

### 1. Indexing
```
File System → Scanner → Parser → Tree → Storage
```

### 2. Context Request
```
Claude Hook → IPC → Handler → Context Manager → Renderer → Response
```

### 3. Experience Grafting
```
Agent Outcome → IPC → Experience Pool → Storage
```

## Performance Targets

| Metric | Target |
|--------|--------|
| Hook response | <5ms P99 |
| Context lookup | <3ms P99 |
| Daemon idle CPU | <1% |
| Daemon memory | <100MB |
| Cache hit rate | >90% |
