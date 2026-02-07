# Phase 3: Context Management

> **Goal**: Build the intelligent context manager that provides relevant, focused context to AI agents using hybrid retrieval.

## Overview

| Aspect | Detail |
|--------|--------|
| **Duration** | 2 weeks |
| **Priority** | High |
| **Dependencies** | Phase 2 (Indexing & Storage) |
| **Deliverables** | Context Manager, Hybrid Router, Context Sandwich Builder |

---

## 3.1 Context Manager

### Objectives
- [x] Create scope for agent sessions
- [x] Mount/unmount context dynamically
- [x] Graft experience from completed agents
- [x] Expand focus on-demand (self-service)

### Core API

```rust
pub struct ContextManager {
    storage: Arc<Storage>,
    projects: Arc<ProjectManager>,
    vector_index: Arc<VectorIndex>,
}

impl ContextManager {
    /// Create a new context scope for an agent
    pub async fn create_scope(&self, req: ScopeRequest) -> Result<ContextScope, Error>;
    
    /// Mount additional context to a scope
    pub async fn expand_focus(&self, scope_id: &str, node_ids: Vec<NodeId>) -> Result<(), Error>;
    
    /// Graft experience from completed agent
    pub async fn graft_experience(&self, cwd: &Path, exp: Experience) -> Result<(), Error>;
    
    /// Get context string for injection
    pub async fn render_context(&self, scope: &ContextScope) -> String;
}
```

### Context Scope Structure

```rust
pub struct ContextScope {
    pub id: String,
    pub project_path: PathBuf,
    
    // Layer 1: Immutable anchor
    pub anchor: AnchorContext,
    
    // Layer 2: Mutable focus area
    pub focus: FocusContext,
    
    // Layer 3: Read-only horizon
    pub horizon: HorizonContext,
}

pub struct AnchorContext {
    pub rules: Vec<String>,           // Project rules
    pub experiences: Vec<Experience>, // Recent decisions
    pub constraints: Vec<String>,     // Parent agent constraints
}

pub struct FocusContext {
    pub primary_nodes: Vec<NodeId>,   // Main focus files
    pub auto_loaded: Vec<NodeId>,     // Dependencies
    pub expanded: Vec<NodeId>,        // User-requested expansions
}

pub struct HorizonContext {
    pub skeleton: SkeletonTree,       // Low-detail global map
    pub hot_nodes: Vec<NodeId>,       // Frequently accessed
}
```

---

## 3.2 Hybrid Retrieval Router

### Objectives
- [x] Classify query intent (structural vs semantic)
- [x] Route to appropriate index (tree-based implemented)
- [ ] Merge results with ranking (RRF deferred to Phase 4)

### Query Classification

| Query Pattern | Route | Example |
|---------------|-------|---------|
| "What calls X?" | Tree (structural) | Dependency graph lookup |
| "How does auth work?" | Vector (semantic) | Embedding similarity |
| "Find X in section Y" | Tree (positional) | Tree traversal |
| "Explain the API" | Hybrid (both) | Merge and rank |

### Router Implementation

```rust
pub struct HybridRouter {
    tree_index: Arc<TreeIndex>,
    vector_index: Arc<VectorIndex>,
    classifier: QueryClassifier,
}

impl HybridRouter {
    pub async fn query(&self, q: &str, scope: &ContextScope) -> Vec<RetrievalResult> {
        let intent = self.classifier.classify(q);
        
        match intent {
            QueryIntent::Structural => self.tree_index.query(q, scope).await,
            QueryIntent::Semantic => self.vector_index.query(q, scope).await,
            QueryIntent::Hybrid => {
                let tree_results = self.tree_index.query(q, scope).await;
                let vector_results = self.vector_index.query(q, scope).await;
                self.merge_rrf(tree_results, vector_results)
            }
        }
    }
    
    /// Reciprocal Rank Fusion for merging results
    fn merge_rrf(&self, a: Vec<RetrievalResult>, b: Vec<RetrievalResult>) -> Vec<RetrievalResult>;
}
```

---

## 3.3 Vector Index

### Objectives
- [ ] Local embeddings (no API calls for speed)
- [ ] Incremental updates
- [ ] Filtered search (by path prefix, language, etc.)

### Implementation Options

| Option | Pros | Cons |
|--------|------|------|
| `usearch` | Lightweight, fast | Fewer features |
| `faiss` | Battle-tested | Larger binary |
| `qdrant` | Full-featured | Requires separate process |

**Recommended**: `usearch` for simplicity, upgrade if needed.

### Embedding Model
- **Model**: `fastembed` with `BAAI/bge-small-en-v1.5`
- **Dimension**: 384
- **Speed**: ~1000 embeddings/sec on CPU

---

## 3.4 Context Sandwich Builder

### Objectives
- [x] Build three-layer context for agents
- [x] Auto-load dependencies for focus files
- [x] Generate low-detail horizon skeleton
- [x] Render to injectable string

### Layer Construction

```rust
impl ContextManager {
    pub async fn build_sandwich(&self, req: SandwichRequest) -> ContextScope {
        // Layer 1: Anchor
        let anchor = self.build_anchor(&req.project_path, &req.parent_constraints).await;
        
        // Layer 2: Focus (with auto-loaded dependencies)
        let focus = self.build_focus(&req.project_path, &req.focus_paths).await;
        
        // Layer 3: Horizon
        let horizon = self.build_horizon(&req.project_path, &focus).await;
        
        ContextScope { anchor, focus, horizon, .. }
    }
    
    async fn build_focus(&self, project: &Path, paths: &[PathBuf]) -> FocusContext {
        let tree = self.projects.get_tree(project).await?;
        
        // Get primary nodes
        let primary: Vec<NodeId> = paths.iter()
            .filter_map(|p| tree.find_node_id_by_path(p))
            .collect();
        
        // Auto-load direct dependencies (1 level)
        let auto_loaded: Vec<NodeId> = primary.iter()
            .flat_map(|id| tree.dependencies.get_imports(*id))
            .collect();
        
        FocusContext { primary_nodes: primary, auto_loaded, expanded: vec![] }
    }
    
    async fn build_horizon(&self, project: &Path, focus: &FocusContext) -> HorizonContext {
        let tree = self.projects.get_tree(project).await?;
        
        // Generate skeleton (directories + file names only)
        let skeleton = tree.to_skeleton_excluding(&focus.all_nodes());
        
        HorizonContext { skeleton, hot_nodes: vec![] }
    }
}
```

### Context Rendering

```rust
impl ContextScope {
    pub fn render(&self) -> String {
        let mut output = String::new();
        
        // Anchor section
        output.push_str("# PROJECT CONTEXT\n\n");
        output.push_str("## Rules\n");
        for rule in &self.anchor.rules {
            output.push_str(&format!("- {}\n", rule));
        }
        
        // Recent experiences
        if !self.anchor.experiences.is_empty() {
            output.push_str("\n## Recent Decisions\n");
            for exp in self.anchor.experiences.iter().take(5) {
                output.push_str(&format!("- {}: {}\n", exp.agent_id, exp.decision));
            }
        }
        
        // Focus section
        output.push_str("\n## Focus Area\n");
        // ... render focus files with content ...
        
        // Horizon section
        output.push_str("\n## Project Structure (overview)\n");
        output.push_str(&self.horizon.skeleton.to_ascii_tree());
        
        output
    }
}
```

---

## 3.5 Experience Pool

### Objectives
- [x] Store agent decisions persistently (JSONL via Storage)
- [x] Load relevant experiences for new sessions
- [ ] Prune old/irrelevant experiences

### Experience Structure

```rust
pub struct Experience {
    pub timestamp: i64,
    pub agent_id: String,
    pub session_id: String,
    pub decision: String,
    pub rationale: Option<String>,
    pub files_touched: Vec<PathBuf>,
    pub outcome: Option<Outcome>,
}

pub enum Outcome {
    Success,
    Failure { error: String },
    Reverted,
}
```

### Grafting Flow

```
SubagentStop hook
      │
      ▼
Parse agent transcript
      │
      ▼
Extract key decisions
      │
      ▼
Write to experience.jsonl
      │
      ▼
Available for next session
```

---

## Testing Requirements

> Reference: [Testing Strategy](./testing-strategy.md) for general guidelines.

### Unit Test Coverage

#### Context Manager

| Component | Required Tests |
|-----------|----------------|
| `create_scope` | Valid request; missing project; invalid paths |
| `expand_focus` | Valid expansion; invalid node IDs; scope not found |
| `graft_experience` | Valid experience; duplicate; malformed data |
| `render_context` | All fields rendered; empty fields; large context |

**Edge cases to cover:**
- Scope for non-initialized project
- Expand focus with node that doesn't exist
- Expand focus with circular dependency
- Graft experience with very large decision text
- Render context when tree is corrupted
- Concurrent scope creation for same project
- Scope eviction under memory pressure
- Context request during project re-indexing

#### Hybrid Router

| Component | Required Tests |
|-----------|----------------|
| QueryClassifier | Each query pattern; ambiguous queries; multi-language |
| Tree queries | Path lookups; dependency traversal; scope filtering |
| Vector queries | Similarity search; filtered search; empty results |
| RRF merging | Score combination; deduplication; rank stability |

**Edge cases to cover:**
- Query with no results from either index
- Query that crashes tree index (fallback to vector)
- Query that crashes vector index (fallback to tree)
- Very long query text (>4KB)
- Query with special characters
- Query in non-English language
- Empty query string
- Query with only stop words

#### Vector Index

| Component | Required Tests |
|-----------|----------------|
| Embedding | All content types; unicode; empty content |
| Indexing | Add; update; delete; batch operations |
| Search | k-NN; filtered; range queries |
| Persistence | Save/load; incremental updates; corruption recovery |

**Edge cases to cover:**
- Embed very short content (<10 chars)
- Embed very long content (>100KB)
- Update embedding for deleted node
- Search with k > total documents
- Concurrent search and update
- Index with single document
- Index with millions of documents (stress)
- Filter that matches nothing

#### Context Sandwich Builder

| Component | Required Tests |
|-----------|----------------|
| `build_anchor` | Rules loaded; experiences loaded; constraints applied |
| `build_focus` | Primary nodes; auto-loaded deps; expanded set |
| `build_horizon` | Skeleton correct; exclusions work; hot nodes |
| `render` | Format correct; size bounded; markdown valid |

**Edge cases to cover:**
- Project with no rules file
- Project with no experiences
- Focus on file with no dependencies
- Focus on file that imports everything
- Horizon for very large project (>100k files)
- Render when some nodes are missing content
- Render when content is binary
- Render size exceeds limit (truncation)

#### Experience Pool

| Component | Required Tests |
|-----------|----------------|
| Store | Write new; concurrent writes; large batch |
| Load | Filter by time; filter by agent; limit count |
| Prune | Age-based; count-based; outcome-based |
| Parse | Extract from transcript; handle malformed |

**Edge cases to cover:**
- Store experience with nil fields
- Store experience with very long rationale
- Load from corrupted JSONL
- Load from empty file
- Prune all experiences
- Parse transcript with no decisions
- Parse transcript with special characters

### Integration Test Coverage

#### Context Flow

```
tests/
├── integration_context.rs
```

Tests must verify:
- Create scope → render context → valid output
- Create scope → expand focus → render updated
- Create scope → request multiple times → consistent
- Graft experience → new scope includes it
- Scope survives project re-indexing

#### Retrieval Flow

```
tests/
├── integration_retrieval.rs
```

Tests must verify:
- Query → correct route decision
- Tree query → structural results
- Vector query → semantic results
- Hybrid query → merged results correct
- Query with scope filtering works
- Performance under repeated queries

#### End-to-End Context Request

```
tests/
├── e2e_context.rs
```

Tests must verify:
- IPC request → scope creation → context returned
- Context includes all three layers
- Context updates when files change
- Context respects expand_focus calls
- Full round-trip latency <5ms

### Performance Test Requirements

> Reference: [Benchmarks Reference](./benchmarks-reference.md) for methodology and sources.

| Metric | Target | Source/Rationale |
|--------|--------|------------------|
| Scope creation (cold project) | <150ms | Includes tree load from disk |
| Scope creation (warm project) | <50ms | Tree cached in memory |
| Context rendering | <10ms | String formatting, concatenation |
| Tree structural query (P50) | <1ms | In-memory graph traversal |
| Tree structural query (P99) | <5ms | Complex dependency chains |
| Vector query 10k docs (P50) | <3ms | USearch k-NN search |
| Vector query 10k docs (P99) | <10ms | With filter overhead |
| Vector query 100k docs (P99) | <30ms | Larger index, HNSW traversal |
| RRF merge (100 results) | <1ms | Simple score combination |
| Hot context (fully cached) | <500µs | In-memory string return |
| Embed single text | <10ms | FastEmbed on CPU |
| Embed batch (100 texts) | <500ms | ~200 texts/sec throughput |

**Performance test structure:**
```rust
#[bench]
fn bench_scope_creation(b: &mut Bencher) {
    // From request to ready scope
}

#[bench]
fn bench_context_render(b: &mut Bencher) {
    // Render full context string
}

#[bench]
fn bench_hybrid_query(b: &mut Bencher) {
    // Full hybrid retrieval
}
```

### Resource Test Requirements

| Resource | Limit | Test |
|----------|-------|------|
| Scope memory | <10MB each | Create scope, measure size |
| Vector index | <500MB | 100k embeddings |
| Context string | <100KB | Render, measure size |
| Experience file | Auto-rotate at 10MB | Long-running test |

**Resource test structure:**
```rust
#[tokio::test]
async fn test_scope_memory_bounded() {
    // Create many scopes
    // Verify total memory stable
}

#[tokio::test]
async fn test_vector_index_size() {
    // Index many documents
    // Verify size reasonable
}
```

### Error Recovery Testing

| Error Scenario | Expected Recovery |
|----------------|-------------------|
| Vector index corruption | Rebuild from tree |
| Experience file corruption | Skip bad lines, continue |
| Missing dependency node | Log warning, exclude |
| Embedding model crash | Fallback to tree-only |
| Scope creation timeout | Return partial context |
| Render exceeds limit | Truncate gracefully |

### Query Classification Testing

Maintain a query test dataset:

```yaml
# test_queries.yaml
queries:
  - input: "What functions call authenticate?"
    expected_route: structural
    expected_contains: ["authenticate", "caller"]
  
  - input: "How does the authentication flow work?"
    expected_route: semantic
    expected_topic: "authentication"
  
  - input: "Find the login function in auth module"
    expected_route: hybrid
```

### Test Execution Commands

```bash
# Context manager tests
cargo test -p engram-core context_manager::

# Router tests
cargo test -p engram-core router::

# Vector index tests
cargo test -p engram-core vector_index::

# Rendering tests
cargo test -p engram-core rendering::

# Integration tests
cargo test --test integration_context
cargo test --test integration_retrieval
cargo test --test e2e_context

# Benchmarks
cargo bench -p engram-core -- context
cargo bench -p engram-core -- retrieval
```

---

## Deliverables Checklist

### Implementation
- [x] ContextManager with scope creation (`engram-context` crate)
- [x] Hybrid router with query classification (tree-based only for now)
- [ ] Local vector index (usearch + fastembed) — **Deferred to Phase 4**
- [x] Context Sandwich builder (anchor/focus/horizon layers)
- [x] Auto-dependency loading
- [x] Experience pool management (graft_experience, JSONL storage)
- [x] Context rendering to injectable strings (ContextRenderer)
- [x] IPC integration (`GetContext`, `PrepareContext`, `GraftExperience`)

### Testing
- [x] Unit tests for context manager (3 tests)
- [x] Unit tests for hybrid router (5 tests)
- [ ] Unit tests for vector index — **Deferred to Phase 4**
- [x] Unit tests for context sandwich (scope: 4 tests)
- [x] Unit tests for experience pool (scope tests)
- [x] Daemon handler context tests (3 tests)
- [x] Integration tests for context flow (6 tests)
- [x] Integration tests for retrieval flow (included above)
- [x] E2E tests for context request (scope creation verified)
- [ ] Performance benchmarks (must meet targets)
- [ ] Resource consumption tests
- [ ] Error recovery tests
- [ ] Query classification test dataset

