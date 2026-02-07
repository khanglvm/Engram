# Phase 5: Optimization & Polish

> **Goal**: Optimize performance, reduce resource usage, and polish the system for production use.

## Overview

| Aspect | Detail |
|--------|--------|
| **Duration** | 1-2 weeks |
| **Priority** | Medium-High |
| **Dependencies** | Phase 4 (Claude Integration) |
| **Deliverables** | Performance optimizations, monitoring, documentation |

---

## 5.1 Memory Optimization

### Objectives
- [ ] Enforce 100MB memory limit
- [ ] Implement memory pressure monitoring
- [ ] Aggressive LRU eviction when needed
- [ ] Measure and reduce allocations

### Strategies

| Strategy | Implementation | Expected Savings |
|----------|----------------|------------------|
| LRU eviction | Unload idle projects after 10min | ~60% |
| Mmap for large data | Tree content via memory-mapped files | Heap → disk |
| String interning | Dedupe repeated strings | ~20% |
| Lazy loading | Load node content on-demand | ~40% |
| Compact serialization | MessagePack instead of JSON | ~30% |

### Memory Monitoring

```rust
pub struct MemoryMonitor {
    limit: usize,
    current: AtomicUsize,
}

impl MemoryMonitor {
    pub fn check_pressure(&self) -> MemoryPressure {
        let usage = self.current.load(Ordering::Relaxed);
        let ratio = usage as f64 / self.limit as f64;
        
        match ratio {
            r if r < 0.7 => MemoryPressure::Normal,
            r if r < 0.9 => MemoryPressure::Warning,
            _ => MemoryPressure::Critical,
        }
    }
    
    pub async fn handle_pressure(&self, manager: &ProjectManager) {
        match self.check_pressure() {
            MemoryPressure::Warning => {
                // Evict least recently used project
                manager.evict_lru().await;
            }
            MemoryPressure::Critical => {
                // Evict all but current project
                manager.evict_all_except_current().await;
            }
            _ => {}
        }
    }
}
```

---

## 5.2 CPU Optimization

### Objectives
- [ ] <1% CPU when idle
- [ ] <5% CPU during active use
- [ ] No busy-waiting or polling

### Strategies

| Area | Optimization |
|------|--------------|
| File watcher | FSEvents (kernel-level, no polling) |
| IPC | Async with tokio (event-driven) |
| Background tasks | Bounded work queue |
| Parsing | Parallel with rayon, but limit threads |

### Background Task Queue

```rust
pub struct TaskQueue {
    sender: mpsc::Sender<Task>,
    // Limit concurrent tasks
    semaphore: Arc<Semaphore>,
}

impl TaskQueue {
    pub fn new(max_concurrent: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel(1000);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        
        // Spawn worker
        tokio::spawn(async move {
            while let Some(task) = receiver.recv().await {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                tokio::spawn(async move {
                    task.execute().await;
                    drop(permit);
                });
            }
        });
        
        Self { sender, semaphore }
    }
    
    pub fn submit(&self, task: Task) {
        let _ = self.sender.try_send(task);
    }
}
```

---

## 5.3 Latency Optimization

### Objectives
- [ ] Hook response <5ms (P99)
- [ ] Context lookup <3ms (P99)
- [ ] Cache hit rate >90%

### Strategies

| Area | Optimization |
|------|--------------|
| IPC | Unix sockets (no network overhead) |
| Serialization | MessagePack (faster than JSON) |
| Caching | Pre-compute context on prepare |
| Response | Return cached, update async |

### Latency Tracking

```rust
#[derive(Default)]
pub struct LatencyTracker {
    samples: RwLock<VecDeque<(String, Duration)>>,
}

impl LatencyTracker {
    pub fn record(&self, operation: &str, duration: Duration) {
        let mut samples = self.samples.write().unwrap();
        samples.push_back((operation.to_string(), duration));
        
        // Keep last 1000 samples
        while samples.len() > 1000 {
            samples.pop_front();
        }
    }
    
    pub fn p99(&self, operation: &str) -> Duration {
        let samples = self.samples.read().unwrap();
        let mut durations: Vec<_> = samples.iter()
            .filter(|(op, _)| op == operation)
            .map(|(_, d)| *d)
            .collect();
        
        durations.sort();
        let idx = (durations.len() as f64 * 0.99) as usize;
        durations.get(idx).copied().unwrap_or_default()
    }
}
```

---

## 5.4 AI Enrichment Optimization

### Objectives
- [ ] Use cheap models for summarization
- [ ] Batch API calls for efficiency
- [ ] Progressive enrichment (priority queue)
- [ ] Cache embeddings to avoid recomputation

### Model Selection

| Task | Recommended Model | Alternative |
|------|------------------|-------------|
| Summaries | GPT-4o-mini | Local Mistral |
| Embeddings | BGE-small (local) | OpenAI ada |
| Classification | Local classifier | GPT-4o-mini |

### Priority Queue for Enrichment

```rust
pub struct EnrichmentQueue {
    queue: BinaryHeap<PrioritizedNode>,
}

impl EnrichmentQueue {
    pub fn prioritize(&mut self, tree: &Tree) {
        // Entry points first
        for node in tree.find_entry_points() {
            self.queue.push(PrioritizedNode {
                priority: 100,
                node_id: node.id,
            });
        }
        
        // High-import-count files next
        for (node_id, count) in tree.dependencies.import_counts() {
            self.queue.push(PrioritizedNode {
                priority: count.min(50) as u8,
                node_id,
            });
        }
        
        // Rest at base priority
        for node in tree.all_nodes() {
            if !self.queue.iter().any(|n| n.node_id == node.id) {
                self.queue.push(PrioritizedNode {
                    priority: 1,
                    node_id: node.id,
                });
            }
        }
    }
}
```

---

## 5.5 Monitoring & Observability

### Objectives
- [ ] Structured logging
- [ ] Metrics collection
- [ ] Health check endpoint
- [ ] Status command output

### Metrics

```rust
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub requests_latency_sum: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub projects_loaded: AtomicU64,
    pub memory_bytes: AtomicU64,
    pub enrichment_pending: AtomicU64,
}

impl Metrics {
    pub fn to_status(&self) -> StatusResponse {
        StatusResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.uptime(),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            cache_hit_rate: self.cache_hit_rate(),
            projects_loaded: self.projects_loaded.load(Ordering::Relaxed) as usize,
            memory_mb: self.memory_bytes.load(Ordering::Relaxed) / 1024 / 1024,
            enrichment_pending: self.enrichment_pending.load(Ordering::Relaxed) as usize,
        }
    }
}
```

### Logging

```rust
// Use tracing with structured fields
tracing::info!(
    project = %project_path.display(),
    files = file_count,
    duration_ms = duration.as_millis(),
    "Project scan completed"
);
```

---

## 5.6 Documentation

### Objectives
- [ ] README with quick start
- [ ] Architecture documentation
- [ ] API reference
- [ ] Troubleshooting guide

### Documentation Structure

```
docs/
├── README.md              # Quick start guide
├── ARCHITECTURE.md        # System design
├── COMMANDS.md            # CLI reference
├── HOOKS.md               # Claude Code hooks reference
├── TROUBLESHOOTING.md     # Common issues
└── DEVELOPMENT.md         # Contributing guide
```

---

## 5.7 Testing Requirements

> Reference: [Testing Strategy](./testing-strategy.md) for general guidelines.
> Reference: [Benchmarks Reference](./benchmarks-reference.md) for performance targets.

### Unit Test Coverage

#### Memory Management

| Component | Required Tests |
|-----------|----------------|
| MemoryMonitor | Pressure levels; threshold transitions; concurrent updates |
| LRU eviction | Evict oldest; evict under pressure; preserve recent |
| Mmap manager | Map/unmap; concurrent access; file change handling |
| String interning | Dedup; lookup; memory savings |

**Edge cases to cover:**
- Memory spike then drop
- Concurrent pressure from multiple projects
- Mmap file deleted while mapped
- String intern table growth
- Memory limit at exact boundary

#### Task Queue

| Component | Required Tests |
|-----------|----------------|
| Submit task | Queue accepts; queue full; priority ordering |
| Execute task | Task completes; task fails; task timeout |
| Concurrency limit | Respects max; no starvation |
| Shutdown | Drain pending; cancel in-flight |

**Edge cases to cover:**
- 1000 tasks queued rapidly
- Task panics
- Task blocks indefinitely
- Shutdown during task execution

#### Latency Tracker

| Component | Required Tests |
|-----------|----------------|
| Record sample | Single; batch; overflow |
| P50/P99 calculation | Accurate percentiles |
| Operation filtering | By operation name |

#### Metrics

| Component | Required Tests |
|-----------|----------------|
| Counters | Increment; overflow; concurrent updates |
| Gauges | Set; increase; decrease |
| Histograms | Bucket distribution |
| Export | JSON format; filter by name |

### Integration Test Coverage

#### Memory Pressure Handling

```
tests/
├── integration_memory.rs
```

Tests must verify:
- Load 3 projects → approach limit → LRU eviction
- Single large project → stays under limit
- Memory recovers after project unload
- No OOM under sustained load

#### CPU Usage

```
tests/
├── integration_cpu.rs
```

Tests must verify:
- Idle daemon: <1% CPU over 60s
- Active daemon (responding): <5% CPU
- Background indexing: <50% CPU (leaves room for user)
- No busy-wait loops

#### Cache Performance

```
tests/
├── integration_cache.rs
```

Tests must verify:
- Cache hit rate >90% under repeated requests
- Cache invalidation on file change
- Cache eviction under memory pressure
- Cache warming on session start

### Performance Test Requirements

> Reference: [Benchmarks Reference](./benchmarks-reference.md) for methodology.

| Metric | Target | Source/Rationale |
|--------|--------|------------------|
| Daemon idle CPU | <1% | Event-driven, no polling |
| Daemon active CPU | <5% | Bounded work queue |
| Daemon idle RSS | <15MB | Minimal structures when no projects |
| Daemon with 1 project RSS | <50MB | Single project loaded |
| Daemon max RSS (3 projects) | <100MB | LRU keeps bounded |
| IPC ping P99 | <500µs | Unix socket + msgpack |
| Context lookup (cached) P99 | <1ms | In-memory string return |
| Cache hit rate | >90% | Pre-computed on prepare |
| Hook response P99 | <5ms | End-to-end from shell |

**Performance test structure:**
```rust
#[tokio::test]
async fn test_idle_cpu_usage() {
    let daemon = start_daemon().await;
    let cpu_before = get_process_cpu_time();
    
    tokio::time::sleep(Duration::from_secs(60)).await;
    
    let cpu_after = get_process_cpu_time();
    let cpu_percent = (cpu_after - cpu_before) / 60.0;
    
    assert!(cpu_percent < 1.0, "Idle CPU too high: {}%", cpu_percent);
}

#[tokio::test]
async fn test_memory_limit_enforcement() {
    let daemon = start_daemon_with_limit(100 * 1024 * 1024).await;
    
    // Load multiple large projects
    for project in large_projects() {
        daemon.init_project(&project).await;
    }
    
    let rss = get_process_rss();
    assert!(rss < 100 * 1024 * 1024, "RSS exceeds limit: {} MB", rss / 1024 / 1024);
}
```

### Resource Test Requirements

| Resource | Limit | Test Method |
|----------|-------|-------------|
| RSS (idle) | <15MB | `ps -o rss` after startup |
| RSS (max) | <100MB | Load 3 large projects |
| Open file handles | <50 | `lsof -p` during operation |
| Threads | <10 | Tokio runtime + rayon |
| CPU (idle) | <1% | 60s measurement |
| CPU (active) | <5% | Under load test |

**Resource test structure:**
```rust
#[tokio::test]
async fn test_file_handle_limit() {
    let daemon = start_daemon().await;
    
    // Perform many operations
    for _ in 0..1000 {
        daemon.ping().await;
    }
    
    let open_files = get_open_file_count();
    assert!(open_files < 50, "Too many open files: {}", open_files);
}
```

### Stress Testing

#### Concurrent Requests

```rust
#[tokio::test]
async fn stress_concurrent_requests() {
    let daemon = start_daemon().await;
    let mut handles = vec![];
    
    // 100 concurrent clients
    for _ in 0..100 {
        let client = daemon.connect().await;
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                client.ping().await.unwrap();
            }
        }));
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify daemon still healthy
    assert!(daemon.status().await.is_ok());
}
```

#### Long-Running Stability

```rust
#[tokio::test]
#[ignore] // Run manually or in CI nightly
async fn stress_long_running() {
    let daemon = start_daemon().await;
    let start_rss = get_process_rss();
    
    // Run for 1 hour with simulated load
    for _ in 0..3600 {
        daemon.simulate_activity().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    
    let end_rss = get_process_rss();
    let growth = end_rss - start_rss;
    
    // Should not grow more than 10MB over 1 hour
    assert!(growth < 10 * 1024 * 1024, "Memory leak detected: {} MB growth", growth / 1024 / 1024);
}
```

### Error Recovery Testing

| Error Scenario | Expected Recovery |
|----------------|-------------------|
| OOM condition | Aggressive eviction, continue |
| Task panic | Log error, continue processing |
| Corrupt cache | Clear and rebuild |
| File system full | Log error, disable writes |
| Resource exhaustion | Graceful degradation |

### Test Execution Commands

```bash
# Unit tests
cargo test -p treerag-daemon optimization::
cargo test -p treerag-core memory::
cargo test -p treerag-core metrics::

# Integration tests
cargo test --test integration_memory
cargo test --test integration_cpu
cargo test --test integration_cache

# Benchmarks
cargo bench --workspace

# Stress tests (CI only)
cargo test --release -- --ignored stress_

# Memory profiling
cargo run --release -- --dev &
ps -o rss,vsz,pid -p $(pgrep treerag-daemon)

# CPU profiling
cargo flamegraph -- treerag-daemon --dev

# Latency testing
hyperfine --warmup 3 'echo "{}" | nc -U /tmp/treerag.sock'
```

---

## Verification

### Performance Validation

```bash
# Run all benchmarks
cargo bench --workspace

# Memory profiling over time
cargo run --release &
for i in {1..60}; do
    ps -o rss -p $(pgrep treerag-daemon) | tail -1
    sleep 1
done

# Latency percentiles
./scripts/measure_latency.sh | percentile --p50 --p99
```

### Production Readiness Checklist

- [ ] All benchmarks pass targets
- [ ] 1-hour stress test passes (no memory leak)
- [ ] 100 concurrent clients handled
- [ ] CPU idle <1% verified
- [ ] Memory limit enforced
- [ ] All error recovery tests pass
- [ ] Documentation complete and accurate
- [ ] Logging produces actionable output

---

## Deliverables Checklist

### Implementation
- [ ] Memory limit enforcement (<100MB)
- [x] Memory pressure monitoring and eviction (`MemoryMonitor`)
- [ ] CPU usage optimization (<1% idle, <5% active)
- [ ] Background task queue with concurrency limit
- [x] Latency tracking with percentiles (`LatencyTracker`)
- [ ] Cache hit rate optimization (>90%)
- [ ] AI enrichment priority queue
- [ ] Structured logging with tracing
- [x] Metrics collection and export (`Metrics` struct)
- [x] Health check endpoint (enhanced Status response with metrics)

### Documentation
- [x] README with quick start (`docs/README.md`)
- [x] Architecture documentation (`docs/ARCHITECTURE.md`)
- [x] CLI/API reference (`docs/COMMANDS.md`)
- [x] Hooks reference (`docs/HOOKS.md`)
- [ ] Troubleshooting guide
- [ ] Development/contributing guide

### Testing
- [ ] Unit tests for memory management
- [ ] Unit tests for task queue
- [x] Unit tests for metrics (6 tests)
- [ ] Integration tests for memory pressure
- [ ] Integration tests for CPU usage
- [ ] Integration tests for cache performance
- [ ] Performance benchmarks (all targets met)
- [ ] Stress tests (concurrent + long-running)
- [ ] Error recovery tests
- [ ] Production readiness checklist verified

