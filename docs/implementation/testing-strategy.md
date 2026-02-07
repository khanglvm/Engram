# Testing Strategy

> **Goal**: Ensure every component is thoroughly tested with comprehensive coverage of edge cases, performance, and resource usage.

---

## 1. Testing Philosophy

### Principles

1. **Test-First Development**: Write tests before or alongside implementation, never after
2. **Edge Case Focus**: Every function must consider boundary conditions and failure modes
3. **Real-World Simulation**: Integration tests must simulate production conditions
4. **Performance as a Feature**: Latency and resource usage are tested, not just correctness
5. **Regression Prevention**: Every bug fix must include a test that would have caught it

### Coverage Requirements

| Level | Target Coverage | Focus |
|-------|-----------------|-------|
| Unit Tests | >90% | Individual functions, error paths, edge cases |
| Integration Tests | >80% | Component interactions, IPC flows, persistence |
| E2E Tests | Key paths | Full user workflows, CLI commands |
| Performance Tests | All hot paths | Latency, throughput, memory |

---

## 2. Unit Test Requirements

### What Must Be Tested

Every function/method must have tests covering:

1. **Happy Path**: Normal expected behavior
2. **Input Validation**: Invalid inputs, empty inputs, null/None values
3. **Boundary Conditions**: Min/max values, empty collections, single elements
4. **Error Paths**: All error variants that can be returned
5. **State Transitions**: Before/after state changes

### Edge Cases Checklist

For **string/path inputs**:
- Empty string `""`
- Very long strings (>4KB, >1MB)
- Unicode characters, emojis
- Path traversal attempts (`../../../etc/passwd`)
- Non-existent paths
- Paths with special characters
- Symlinks (circular, broken)
- Paths with spaces and quotes

For **numeric inputs**:
- Zero
- Negative numbers (if applicable)
- Maximum values (`u64::MAX`, `usize::MAX`)
- Overflow scenarios
- NaN/Infinity for floats

For **collections**:
- Empty collection
- Single element
- Very large collections (10k+ items)
- Duplicate elements
- Concurrent modification (if applicable)

For **async operations**:
- Timeout scenarios
- Cancellation mid-operation
- Concurrent execution
- Order of completion

### Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // Group tests by function/method
    mod function_name {
        use super::*;
        
        #[test]
        fn happy_path() { /* ... */ }
        
        #[test]
        fn empty_input() { /* ... */ }
        
        #[test]
        fn invalid_input() { /* ... */ }
        
        #[test]
        fn boundary_max() { /* ... */ }
        
        #[test]
        fn error_case_x() { /* ... */ }
    }
}
```

---

## 3. Integration Test Requirements

### Scope

Integration tests verify component interactions:

1. **IPC Communication**: Client ↔ Server full round-trips
2. **Storage Layer**: Read/write cycles, persistence across restarts
3. **Daemon Lifecycle**: Start, operate, shutdown sequences
4. **Project Management**: Init, load, unload, eviction cycles
5. **File System**: Watcher triggers, index updates

### Test Isolation

Each integration test must:

1. **Use isolated resources**: Unique socket paths, temp directories
2. **Clean up after itself**: Delete files, stop processes
3. **Not depend on order**: Tests run in any order
4. **Not depend on timing**: Use explicit waits, not sleeps

### Concurrency Testing

All shared resources must be tested for:

1. **Race conditions**: Multiple clients, simultaneous operations
2. **Deadlocks**: Lock ordering, async task interactions
3. **Data integrity**: Concurrent reads/writes don't corrupt
4. **Resource leaks**: Connections, file handles, memory

```rust
#[tokio::test]
async fn test_concurrent_clients() {
    let server = spawn_test_server().await;
    
    // Spawn N concurrent clients
    let handles: Vec<_> = (0..100)
        .map(|_| tokio::spawn(async move {
            let client = connect_to_server().await;
            client.request(Request::Ping).await
        }))
        .collect();
    
    // All should succeed without errors
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }
}
```

---

## 4. Performance Test Requirements

> **Important**: All performance targets are derived from published benchmarks and real-world measurements.
> See [Benchmarks Reference](./benchmarks-reference.md) for sources and methodology.

### Latency Testing

Every hot path must have latency benchmarks:

| Path | Target | Measurement |
|------|--------|-------------|
| IPC ping round-trip | <500µs (P99) | Request → Response (Unix socket) |
| IPC request with 1KB payload | <1ms (P99) | Including serialization |
| Hook execution | <5ms (P99) | Shell script start → exit |
| Context lookup (cached) | <1ms (P99) | Request → context string |
| Context lookup (cold) | <50ms (P99) | Including project load |
| File change handling | <50ms (P99) | FSEvent → index update |
| Tree-sitter parse (small file) | <5ms | <100 lines |
| Tree-sitter parse (medium file) | <20ms | 100-1000 lines |
| Vector search (10k docs) | <10ms (P99) | k-NN with USearch |

### How to Test Latency

```rust
#[bench]
fn bench_ipc_roundtrip(b: &mut Bencher) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(spawn_test_server());
    
    b.iter(|| {
        rt.block_on(async {
            let mut client = connect().await.unwrap();
            client.send(Request::Ping).await.unwrap()
        })
    });
}

// Also measure percentiles
fn measure_latency_distribution(n: usize) -> LatencyStats {
    let mut samples = Vec::with_capacity(n);
    
    for _ in 0..n {
        let start = Instant::now();
        // ... operation ...
        samples.push(start.elapsed());
    }
    
    samples.sort();
    LatencyStats {
        p50: samples[n / 2],
        p90: samples[n * 90 / 100],
        p99: samples[n * 99 / 100],
        max: samples[n - 1],
    }
}
```

### Throughput Testing

Test maximum operations per second:

```rust
#[tokio::test]
async fn test_throughput() {
    let server = spawn_test_server().await;
    
    let start = Instant::now();
    let n = 10_000;
    
    for _ in 0..n {
        client.request(Request::Ping).await.unwrap();
    }
    
    let elapsed = start.elapsed();
    let ops_per_sec = n as f64 / elapsed.as_secs_f64();
    
    assert!(ops_per_sec > 5000.0, "Expected >5000 ops/sec, got {}", ops_per_sec);
}
```

---

## 5. Resource Test Requirements

### Memory Testing

Every component must be tested for memory behavior:

1. **Bounded Memory**: Operations don't grow memory unboundedly
2. **Leak Detection**: Long-running tests don't leak
3. **Peak Usage**: Track maximum memory during stress
4. **GC Behavior**: LRU eviction works correctly

```rust
#[test]
fn test_memory_bounded() {
    let manager = ProjectManager::new(&config);
    
    // Load many more projects than cache size
    for i in 0..100 {
        manager.init_project(&project_path(i)).await;
    }
    
    // Should only have max_projects in memory
    assert_eq!(manager.loaded_count().await, config.max_projects);
}

#[test]
fn test_no_memory_leak() {
    let initial_mem = get_memory_usage();
    
    // Run many operations
    for _ in 0..1000 {
        let _project = manager.get_project(&path).await;
        // Project goes out of scope
    }
    
    // Force cleanup
    manager.gc().await;
    
    let final_mem = get_memory_usage();
    let growth = final_mem - initial_mem;
    
    // Memory growth should be minimal
    assert!(growth < 1_000_000, "Memory grew by {} bytes", growth);
}
```

### File Descriptor Testing

Test that file handles are properly managed:

```rust
#[tokio::test]
async fn test_no_fd_leak() {
    let initial_fds = count_open_fds();
    
    // Many connections
    for _ in 0..1000 {
        let mut client = connect().await.unwrap();
        client.send(Request::Ping).await.unwrap();
        // Client dropped
    }
    
    let final_fds = count_open_fds();
    
    // Should not accumulate file descriptors
    assert!(final_fds - initial_fds < 10);
}
```

### CPU Testing

Verify CPU usage targets:

```rust
#[tokio::test]
async fn test_idle_cpu_usage() {
    let server = spawn_daemon().await;
    
    // Let it idle for 5 seconds
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Measure CPU for next 5 seconds
    let cpu_usage = measure_process_cpu(server.pid(), Duration::from_secs(5));
    
    assert!(cpu_usage < 1.0, "Idle CPU should be <1%, got {}%", cpu_usage);
}
```

---

## 6. Error Handling Test Requirements

### Error Path Coverage

Every error variant must be:

1. **Triggerable**: Test must be able to induce the error
2. **Recoverable**: System returns to valid state after error
3. **Reportable**: Error message is useful for debugging
4. **Propagated correctly**: Error reaches the right handler

### Failure Injection

Test behavior under failures:

```rust
// File system failures
#[test]
fn test_readonly_filesystem() {
    // Make directory read-only
    set_readonly(&temp_dir);
    
    let result = storage.save(&data);
    assert!(matches!(result, Err(StorageError::Io(_))));
    
    // Should not corrupt existing data
    assert!(existing_data_intact(&temp_dir));
}

// Network/IPC failures
#[tokio::test]
async fn test_client_disconnect_midrequest() {
    let server = spawn_server().await;
    
    // Connect and send partial request
    let stream = connect_raw().await;
    stream.write(&partial_request).await;
    drop(stream); // Disconnect mid-request
    
    // Server should handle gracefully
    assert!(server.is_healthy().await);
}

// Resource exhaustion
#[test]
fn test_out_of_disk_space() {
    // Fill temp filesystem
    let result = storage.save(&large_data);
    assert!(matches!(result, Err(StorageError::Io(_))));
}
```

---

## 7. Best Practices

### Test Naming

```rust
// Pattern: test_<unit>_<scenario>_<expected_behavior>
#[test]
fn test_project_manager_empty_cache_returns_none() { }

#[test]
fn test_ipc_client_timeout_returns_error() { }

#[test]
fn test_scanner_symlink_loop_does_not_hang() { }
```

### Test Documentation

```rust
/// Tests that the LRU cache properly evicts the oldest project
/// when the cache is full and a new project is loaded.
/// 
/// This is critical for memory bounds enforcement.
#[test]
fn test_lru_eviction_on_capacity() {
    // ...
}
```

### Test Independence

```rust
// BAD: Tests share state
static SHARED_SERVER: OnceCell<Server> = OnceCell::new();

// GOOD: Each test creates its own resources
#[tokio::test]
async fn test_something() {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test.sock");
    let server = spawn_server(&socket_path).await;
    // ...
}
```

### Flaky Test Prevention

```rust
// BAD: Race condition
tokio::time::sleep(Duration::from_millis(100)).await;
assert!(server_ready);

// GOOD: Explicit readiness check
wait_for_condition(
    || server.is_ready(),
    Duration::from_secs(5),
    "Server should be ready"
).await?;
```

---

## 8. Test Organization

### Directory Structure

```
crates/
├── engram-ipc/
│   ├── src/
│   │   └── *.rs
│   └── tests/
│       ├── integration_ipc.rs      # Client-server tests
│       └── stress_test.rs          # Load testing
│
├── engram-core/
│   ├── src/
│   │   └── *.rs
│   └── tests/
│       ├── integration_storage.rs  # Persistence tests
│       └── integration_manager.rs  # Project lifecycle
│
└── engram-daemon/
    ├── src/
    │   └── *.rs
    └── tests/
        ├── integration_daemon.rs   # Full daemon tests
        └── e2e_cli.rs              # CLI command tests

tests/                              # Workspace-level tests
├── common/
│   └── mod.rs                      # Shared test utilities
├── performance/
│   ├── latency_tests.rs
│   └── throughput_tests.rs
└── stress/
    ├── concurrent_clients.rs
    └── memory_pressure.rs
```

### Shared Test Utilities

```rust
// tests/common/mod.rs

/// Spawn a test server with unique socket path
pub async fn spawn_test_server() -> TestServer {
    let temp_dir = tempdir().unwrap();
    let socket_path = temp_dir.path().join("test.sock");
    // ...
}

/// Wait for a condition with timeout
pub async fn wait_for<F, Fut>(
    condition: F,
    timeout: Duration,
    msg: &str,
) -> Result<(), Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{ /* ... */ }

/// Get current memory usage of this process
pub fn get_memory_usage() -> usize { /* ... */ }

/// Count open file descriptors
pub fn count_open_fds() -> usize { /* ... */ }
```

---

## 9. CI/CD Integration

### Required Checks

Every PR must pass:

1. `cargo test --workspace` - All tests pass
2. `cargo clippy --workspace` - No warnings
3. `cargo fmt --check` - Code is formatted
4. `cargo bench` - Performance not regressed (vs main branch)

### Test Categories in CI

```yaml
# .github/workflows/test.yml
jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --workspace --lib
  
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --workspace --test '*'
  
  performance-tests:
    runs-on: ubuntu-latest
    steps:
      - run: cargo bench -- --save-baseline pr
      - run: cargo bench -- --baseline main --compare
```

---

## 10. Phase-Specific Testing Appendix

Each phase document includes a testing section that references this strategy and adds phase-specific requirements:

- [Phase 1: Daemon Foundation - Testing Requirements](./phase-1-daemon-foundation.md#testing-requirements)
- [Phase 2: Indexing & Storage - Testing Requirements](./phase-2-indexing-storage.md#testing-requirements)
- [Phase 3: Context Management - Testing Requirements](./phase-3-context-management.md#testing-requirements)
- [Phase 4: Claude Integration - Testing Requirements](./phase-4-claude-integration.md#testing-requirements)
- [Phase 5: Optimization & Polish - Testing Requirements](./phase-5-optimization.md#testing-requirements)

### Performance Targets Source

All performance targets in phase documents are derived from the [Benchmarks Reference](./benchmarks-reference.md), which documents:

- **Source data** from published benchmarks
- **Realistic targets** for each component
- **Methodology** for measurement
- **Hardware considerations** for CI/CD and local development

