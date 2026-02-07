# Phase 2: Indexing & Storage

> **Goal**: Implement the core indexing engine that scans repositories, builds tree structures, and persists data efficiently.

## Overview

| Aspect | Detail |
|--------|--------|
| **Duration** | 2-3 weeks |
| **Priority** | Critical |
| **Dependencies** | Phase 1 (Daemon Foundation) |
| **Deliverables** | Fast scanner, tree builder, persistence layer, file watcher |

---

## 2.1 Fast Algorithmic Scanner

### Objectives
- [x] File system walker with gitignore support
- [x] Language detection (extension-based)
- [x] AST parsing for code structure extraction
- [x] Framework/pattern detection
- [ ] <30 second target for typical projects (benchmarks pending)

### Scanner Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          SCANNER PIPELINE                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│  File Walker ──► Lang Detect ──► AST Parser ──► Tree Build                 │
│  (parallel)      (fast)          (parallel)     (aggregate)                │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Key Components

1. **File Walker**: Use `ignore` crate for parallel walking with .gitignore support
2. **Language Detection**: Extension-based with fallback to magic bytes
3. **AST Parsing**: `tree-sitter` for each supported language
4. **Framework Detection**: Pattern matching on config files

### Supported Languages (Initial)
- TypeScript/JavaScript
- Python
- Rust
- Go

---

## 2.2 Tree Structure

### Node Types
- `Directory`: Folder in the tree
- `File`: Source file with language info
- `Symbol`: Function, class, interface, etc.

### Key Fields per Node
- `id`: Unique NodeId
- `name`: Display name
- `path`: Relative path from root
- `children`: Child nodes
- `content`: Optional (symbols, hash, line count)
- `summary`: AI-generated (empty initially)
- `tags`: Inferred labels

### Dependency Graph
- Forward edges: File A imports File B
- Reverse edges: What imports File A (for impact analysis)

---

## 2.3 Persistence Layer

### Storage Layout
```
~/.treerag/projects/<hash>/
├── manifest.json       # Project metadata
├── skeleton.json       # Fast-load structure (no content)
├── enriched.json       # Full AI-enriched data
├── dependencies.json   # Dependency graph
├── experience.jsonl    # Append-only decisions log
└── snapshots/          # Historical versions
```

### Key Operations
1. **save_skeleton**: Fast JSON save of structure only
2. **save_enriched**: Full tree with mmap support
3. **load_skeleton**: Fast initial load
4. **load_tree_mmap**: Memory-mapped lazy access
5. **append_experience**: Append to JSONL log
6. **create_snapshot**: Timestamped backup

---

## 2.4 File Watcher

### Requirements
- Use FSEvents on macOS (via `notify` crate)
- Debounce rapid changes (500ms window)
- Non-blocking (run in background task)
- Trigger incremental re-indexing

### Incremental Re-indexing
- Parse only changed file
- Update corresponding tree node
- Clear AI summary (needs re-enrichment)
- Update dependency edges

---

## Testing Requirements

> Reference: [Testing Strategy](./testing-strategy.md) for general guidelines.

### Unit Test Coverage

#### Scanner Module

| Component | Required Tests |
|-----------|----------------|
| File Walker | Parallel traversal; gitignore respect; symlink handling; permission errors |
| Language Detection | All supported extensions; ambiguous extensions; no extension; binary files |
| AST Parser | Each language; syntax errors; very large files; unicode identifiers |
| Framework Detection | Each framework pattern; false positive prevention; multiple frameworks |

**Edge cases to cover:**
- Empty directory
- Directory with only hidden files
- Very deep directory nesting (>100 levels)
- Circular symlinks
- Broken symlinks
- Files with no read permission
- Binary files misidentified as source
- Very long file paths (>4096 chars)
- File changes during scan (race conditions)
- UTF-16/UTF-32 encoded source files
- Files with mixed line endings
- Extremely large single file (>100MB)
- Thousands of small files in single directory

#### Tree Structure

| Component | Required Tests |
|-----------|----------------|
| Node Creation | All node types; required fields; optional fields |
| Tree Building | Parent-child relationships; path calculation; ID uniqueness |
| Dependency Graph | Forward edges; reverse edges; cycle detection |
| Serialization | JSON round-trip; MessagePack round-trip; version compatibility |

**Edge cases to cover:**
- Empty tree
- Single file project
- Very deep tree (>1000 levels)
- Very wide tree (>10000 siblings)
- Node with duplicate name in same directory
- Special characters in names
- Very long node names
- Circular dependencies in code

#### Persistence Layer

| Component | Required Tests |
|-----------|----------------|
| save_skeleton | Correct structure; no content leakage; atomic write |
| save_enriched | All fields present; mmap-compatible format |
| load_skeleton | Fast load; version check; migration |
| load_tree_mmap | Lazy access works; concurrent reads |
| append_experience | Atomic append; concurrent writes; file rotation |
| create_snapshot | Timestamped correctly; full data preserved |

**Edge cases to cover:**
- Write during read
- Concurrent writes to same file
- Disk full during write
- Power failure mid-write (atomic commit check)
- Corrupted file recovery
- Very large tree (>1GB serialized)
- Read from mmap while file is being updated
- Missing parent directories
- Snapshot during active writes

#### File Watcher

| Component | Required Tests |
|-----------|----------------|
| Event Detection | Create; modify; delete; rename |
| Debouncing | Rapid changes coalesced; timeout respected |
| Background Task | Non-blocking; cancellation works |
| Incremental Index | Only changed files parsed; parents updated |

**Edge cases to cover:**
- File created then immediately deleted
- Rename across directories
- Directory deleted (all children events)
- Editor temp file patterns (.swp, ~, .bak)
- Rapid successive changes to same file
- Move directory with many files
- .gitignore changes (should re-evaluate visibility)
- Watch limit exceeded (many directories)

### Integration Test Coverage

#### Full Scan Pipeline

```
tests/
├── integration_scanner.rs
```

Tests must verify:
- Empty repo → empty tree
- Git repo → respects .gitignore
- Monorepo → multiple project detection
- Scan → persist → load → verify identical
- Re-scan after changes → correct deltas
- Concurrent scan requests → queued correctly
- Cancel during scan → partial results handled

#### Persistence Round-Trip

```
tests/
├── integration_storage.rs
```

Tests must verify:
- save → load preserves all data
- mmap access is consistent with full load
- Experience log grows correctly
- Snapshots can be restored
- Version migration from old format
- Recovery from corrupted state

#### File Watcher Integration

```
tests/
├── integration_watcher.rs
```

Tests must verify:
- File change → event received → index updated
- Multiple changes → debounced correctly
- Watcher + scanner don't conflict
- Watcher respects gitignore
- Restart watcher after crash

### Performance Test Requirements

> Reference: [Benchmarks Reference](./benchmarks-reference.md) for methodology and sources.

| Metric | Target | Source/Rationale |
|--------|--------|------------------|
| Walk 1k files (warm cache) | <200ms | ~5k files/sec with gitignore checks |
| Walk 10k files (warm cache) | <2s | Parallel walking, metadata collection |
| Walk 10k files (cold cache) | <10s | Disk I/O dominates on first scan |
| Walk 100k files (warm cache) | <20s | Linear scaling with parallelism |
| Parse small file (<100 lines) | <5ms | Tree-sitter initial parse |
| Parse medium file (100-1000 lines) | <20ms | Typical source file |
| Parse large file (1000-10000 lines) | <100ms | Large modules |
| Incremental re-parse (edit) | <5ms | Tree-sitter incremental update |
| Tree serialization (10k nodes) | <500ms | JSON with serde |
| Tree skeleton load | <50ms | JSON parse, 100KB-1MB |
| Tree mmap initial access | <10ms | Memory mapping overhead |
| File event → index update | <50ms | Debounce + incremental parse |

**Performance test structure:**
```rust
#[bench]
fn bench_scan_10k_files(b: &mut Bencher) {
    // Use realistic project structure
}

#[bench]
fn bench_parse_typescript(b: &mut Bencher) {
    // Various file sizes
}

#[bench]
fn bench_tree_serialize_json(b: &mut Bencher) {
    // 10k node tree
}

#[bench]
fn bench_incremental_reindex(b: &mut Bencher) {
    // Single file change
}
```

### Resource Test Requirements

| Resource | Limit | Test |
|----------|-------|------|
| Memory during scan | <500MB | Scan 100k file project |
| Memory after scan | <50MB (idle) | Tree evicted from cache |
| CPU during scan | <100% per core | Parallel utilization |
| Disk write | <2x source size | Storage overhead |
| File handles | <100 during scan | Limit open handles |

**Resource test structure:**
```rust
#[tokio::test]
async fn test_memory_during_large_scan() {
    // Monitor peak RSS during scan
    // Verify under limit
}

#[tokio::test]
async fn test_scan_parallel_efficiency() {
    // Verify using multiple cores
    // But not thrashing
}
```

### Error Recovery Testing

| Error Scenario | Expected Recovery |
|----------------|-------------------|
| Parser crash on malformed file | Skip file, log warning, continue |
| Disk full during persist | Transaction rollback, clear error |
| Permission denied on file | Skip file, include in warnings |
| Tree-sitter timeout | Fallback to basic analysis |
| Watcher event overflow | Full re-scan triggered |
| Mmap file deleted | Re-load from JSON backup |

### Test Data Sets

Maintain test fixtures for:

1. **Minimal**: 5 files, 1 language
2. **Small**: 100 files, 2 languages
3. **Medium**: 1k files, 4 languages, monorepo
4. **Large**: 10k files, mixed (use on CI only)
5. **Pathological**: Edge cases (deep nesting, huge files, etc.)

### Test Execution Commands

```bash
# Scanner tests
cargo test -p treerag-indexer scanner::

# Storage tests
cargo test -p treerag-core storage::

# Watcher tests
cargo test -p treerag-indexer watcher::

# Integration tests
cargo test --test integration_scanner
cargo test --test integration_storage
cargo test --test integration_watcher

# Benchmarks
cargo bench -p treerag-indexer -- scanner
cargo bench -p treerag-core -- storage

# Large dataset tests (CI only)
cargo test --release -- --ignored large_dataset
```

---

## Deliverables Checklist

### Implementation
- [x] Fast file walker with gitignore
- [x] Tree-sitter parsers for 4+ languages
- [x] Framework detection
- [x] Tree data structure with dependency graph
- [x] JSON + MessagePack persistence
- [ ] Memory-mapped file access (deferred: mmap optional optimization)
- [x] Experience log (JSONL)
- [x] File watcher with debouncing
- [ ] Incremental re-indexing (daemon integration pending)

### Testing
- [x] Unit tests for scanner (workspace total: 114+ tests passing)
- [x] Unit tests for tree structure
- [x] Unit tests for persistence
- [x] Unit tests for file watcher
- [x] Integration tests for scan pipeline (8 tests)
- [x] Integration tests for persistence round-trip (included above)
- [x] Integration tests for file watcher (included above)
- [ ] Performance benchmarks (must meet targets)
- [ ] Resource consumption tests
- [ ] Error recovery tests
- [ ] Test fixtures for all sizes

