# Performance Benchmarks Reference

> **Purpose**: Document realistic performance targets based on actual benchmarks and industry data.

This document provides evidence-based performance targets for Engram components. All numbers are derived from published benchmarks, library documentation, or real-world measurements.

---

## 1. IPC Performance (Unix Domain Sockets + Tokio)

### Source Data
- Rust IPC with memory-mapped ring buffers + Tokio: **800ns median, <1.2µs P99** for 256-byte payloads [gitconnected.com]
- Tokio networking adds ~8µs overhead for localhost TCP [zenoh.io]
- Unix domain sockets bypass network stack, performing better than TCP for local IPC

### Realistic Targets for Engram

| Metric | Target | Rationale |
|--------|--------|-----------|
| Ping round-trip (P50) | <100µs | UDS with MessagePack serialization adds overhead vs raw ring buffers |
| Ping round-trip (P99) | <500µs | Account for scheduling jitter, GC-like effects from allocator |
| Request with payload (P50) | <200µs | 1-4KB typical request size |
| Request with payload (P99) | <1ms | Larger payloads, serialization overhead |
| Fire-and-forget ack | <50µs | Minimal processing, immediate ack |

### Notes
- MessagePack serialization adds ~10-50µs per KB vs raw bytes
- Connection establishment: ~100-200µs (reuse connections where possible)
- Context switches: ~1-5µs on modern CPUs

---

## 2. File System Walking

### Source Data
- `ignore` crate (ripgrep's walker): Parallel traversal with gitignore support
- `srusty-files`: Claims 10,000+ files/second on SSD [crates.io]
- `walkdir`: Performance comparable to system `find` [github.com/BurntSushi/walkdir]
- File system I/O is the bottleneck; actual speeds depend on disk and cache state

### Realistic Targets for Engram

| Metric | Target | Rationale |
|--------|--------|-----------|
| Walk 1k files (warm cache) | <200ms | ~5,000 files/sec with gitignore checks |
| Walk 10k files (warm cache) | <2s | Parallel walking, metadata collection |
| Walk 10k files (cold cache) | <10s | Disk I/O dominates |
| Walk 100k files (warm cache) | <20s | Linear scaling with parallelism |

### Notes
- Cold cache: First scan after reboot or cache eviction
- Warm cache: Subsequent scans, OS has cached directory entries
- SSD vs HDD: 5-10x difference expected
- Network filesystems (NFS, CIFS): Significantly slower, 10-100x

---

## 3. Tree-sitter Parsing

### Source Data
- Designed for editor keystroke-speed: Parse chunks of ~3ms to maintain UI responsiveness [pulsar-edit.dev]
- Node lookup: Sub-microsecond ("insanely fast") [reddit]
- Full file parse: Varies by language and file size
- Incremental parsing: Sub-millisecond for small edits

### Realistic Targets for Engram

| Metric | Target | Rationale |
|--------|--------|-----------|
| Parse small file (<100 lines) | <5ms | Initial parse, AST construction |
| Parse medium file (100-1000 lines) | <20ms | Typical source file |
| Parse large file (1000-10000 lines) | <100ms | Large modules/classes |
| Parse very large file (>10k lines) | <500ms | Edge case, consider chunking |
| Incremental re-parse (edit) | <5ms | Only reparse changed region |
| Node lookup | <10µs | Tree traversal to find node |

### Notes
- Language complexity varies: TypeScript/Python faster than C++ templates
- Very large files (>100KB source): May need streaming or chunked parsing
- Syntax errors: Parser continues but may be slower

---

## 4. Vector Search (USearch)

### Source Data
- Small collections: 2.54ms search latency [github.com/unum-cloud/usearch]
- Large scale: 150k-225k QPS for 32-bit float vectors [unum.cloud]
- With 8-bit quantization: Up to 274k QPS [github.com]
- FAISS IndexFlatL2: 55.3ms (USearch is 20x faster for brute-force)

### Realistic Targets for Engram

| Metric | Target | Rationale |
|--------|--------|-----------|
| Search 1k vectors (k=10) | <1ms | In-memory, SIMD-optimized |
| Search 10k vectors (k=10) | <3ms | Linear scale, still fast |
| Search 100k vectors (k=10) | <10ms | May need HNSW index |
| Search 1M vectors (k=10) | <50ms | Requires proper indexing |
| Add single vector | <100µs | Append operation |
| Batch add 1k vectors | <10ms | Bulk insert |
| Index build (10k vectors) | <1s | Initial index construction |
| Index build (100k vectors) | <10s | Larger index |

### Notes
- Dimensions: 384 (bge-small) - higher dims = slower search
- Filter queries: Add 10-50% overhead
- Disk-backed index: 2-5x slower than in-memory

---

## 5. Embedding Generation (FastEmbed)

### Source Data
- bge-small-en-v1.5 on 24-core AMD Ryzen 9 7900X: ~190 sequences/sec [doughanley.com]
- Uses ONNX Runtime with quantized models [qdrant.tech]
- Quantized models: Up to 4x speedup vs bf16 baseline [huggingface.co]
- Dimension: 384 for bge-small-en-v1.5

### Realistic Targets for Engram

| Metric | Target | Rationale |
|--------|--------|-----------|
| Embed single short text (<100 tokens) | <10ms | Single inference on decent CPU |
| Embed single long text (512 tokens) | <20ms | Max context length |
| Batch embed 10 texts | <50ms | Batched inference |
| Batch embed 100 texts | <500ms | ~200 texts/sec |
| Throughput (sustained) | 100-200 texts/sec | CPU-only, varies by hardware |

### Notes
- GPU: 10-50x faster if available
- First inference: Cold start adds 100-500ms (model loading)
- Text truncation: Texts > 512 tokens need chunking
- Lower-end CPUs: Expect 50-100 texts/sec

---

## 6. Memory Usage

### Source Data
- Minimal idle Rust process: ~644KB [github.com benchmark]
- Typical Rust application startup: 10-15MB [rust-lang.org forum]
- Static linking increases binary size but not runtime RSS
- Each loaded library adds to footprint

### Realistic Targets for Engram

| Component | Target | Rationale |
|-----------|--------|-----------|
| Daemon idle (no projects) | <15MB | Tokio runtime + basic structures |
| Daemon with 1 project loaded | <50MB | Project tree, manifest, caches |
| Daemon with 3 projects (max) | <100MB | LRU keeps memory bounded |
| Vector index (10k docs) | <50MB | 384-dim * 10k * 4 bytes + overhead |
| Vector index (100k docs) | <200MB | Scales linearly |
| Peak during scan | <500MB | Temporary buffers, AST nodes |

### Notes
- RSS vs VSZ: RSS is actual physical memory; VSZ includes mapped files
- mmap files: Don't count toward RSS until accessed
- Memory fragmentation: Long-running processes may show higher RSS

---

## 7. Latency Budgets

### Hook Latency Target: <5ms (P99)

This is the critical path for Claude Code integration. Budget breakdown:

| Step | Budget | Notes |
|------|--------|-------|
| Shell script startup | <1ms | Bash/zsh minimal overhead |
| IPC send/receive | <0.5ms | Unix socket, small payload |
| Cache lookup | <0.1ms | In-memory LRU |
| Context rendering (cached) | <1ms | String formatting |
| Return to hook | <0.5ms | Response serialization |
| **Total budget** | <3ms | **2ms margin for safety** |

### Context Request Latency Target: <50ms (P99)

Full context retrieval when not cached:

| Step | Budget | Notes |
|------|--------|-------|
| IPC | <1ms | Request/response |
| Project load (if not cached) | <100ms | From disk (amortized) |
| Scope creation | <10ms | Layer assembly |
| Dependency resolution | <10ms | Graph traversal |
| Content loading | <20ms | File reads |
| Rendering | <5ms | String formatting |
| **Total (cold)** | <150ms | First request |
| **Total (warm)** | <50ms | Subsequent requests |

---

## 8. Throughput Targets

| Operation | Target | Measurement |
|-----------|--------|-------------|
| IPC requests/sec (sustained) | >10,000 | Concurrent clients |
| File scans/sec (small files) | >5,000 | Parallel walker |
| Tree nodes processed/sec | >100,000 | Traversal + serialization |
| Vector searches/sec | >1,000 | k-NN with small index |
| Context renders/sec | >1,000 | Cached project |

---

## 9. Disk I/O

| Operation | Target | Notes |
|-----------|--------|-------|
| Project manifest read | <5ms | JSON, typically <10KB |
| Project manifest write | <10ms | Atomic write |
| Tree skeleton load | <50ms | JSON, 100KB-1MB typical |
| Tree full load (mmap) | <10ms | Just mapping, not reading |
| Experience append | <1ms | JSONL line append |
| Snapshot create | <1s | Full project copy |

---

## 10. Test Environment Considerations

### CI/CD
- GitHub Actions runners: 2-core, 7GB RAM, SSD
- Expect 2-3x slower than local development machine
- Set CI-specific timeouts: 2x local targets

### Local Development
- Minimum: 4-core CPU, 8GB RAM, SSD
- Recommended: 8-core CPU, 16GB RAM, NVMe SSD
- Tests should pass on minimum spec

### Hardware Variance
- CPU: 2-5x variance between low-end and high-end
- Disk: 10-100x variance between NVMe and HDD
- Memory: Speed matters less, but capacity affects cache behavior

---

## References

1. Unix socket IPC benchmarks: https://gitconnected.com/
2. Tokio networking latency: https://zenoh.io/blog/
3. USearch benchmarks: https://github.com/unum-cloud/usearch
4. FastEmbed performance: https://qdrant.tech/articles/fastembed/
5. Tree-sitter design: https://tree-sitter.github.io/
6. Rust memory profiling: https://nnethercote.github.io/perf-book/
7. walkdir benchmarks: https://github.com/BurntSushi/walkdir
