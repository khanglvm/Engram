# Technical Deep-Dive: Core Patterns

> Detailed implementation patterns for critical components.

---

## 1. Non-Blocking Hook Pattern

**Problem**: Claude Code hooks must return in <5ms to avoid blocking the AI thread.

**Solution**: Fire-and-forget with pre-computed cache.

```
┌──────────────────────────────────────────────────────────────────┐
│                    NON-BLOCKING HOOK FLOW                        │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Hook Call (T=0ms)                                               │
│       │                                                          │
│       ├── Read from cache file (synchronous, <1ms)               │
│       │       └── /tmp/engram_cache/<hash>.ctx                  │
│       │                                                          │
│       ├── Return cached context immediately (T=1ms)              │
│       │                                                          │
│       └── Fire-and-forget: nc -U -w0 $SOCKET &  (T=2ms)          │
│               │                                                  │
│               └── Daemon receives request (async)                │
│                       └── Prepares fresh context                 │
│                       └── Writes to cache file                   │
│                       └── Ready for NEXT hook call               │
│                                                                  │
│  Hook Returns (T=3ms) ✓                                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### Cache File Format

```json
{
  "timestamp": 1706940000,
  "project_hash": "a1b2c3d4",
  "context": "# PROJECT CONTEXT\n\n## Rules\n- ...",
  "nodes_included": ["src/main.rs", "src/lib.rs"],
  "experience_count": 5
}
```

---

## 2. Memory-Mapped Tree Access

**Problem**: Large trees (10MB+) should not consume heap memory.

**Solution**: Memory-mapped files with lazy deserialization.

### File Structure

```
tree.mmap
├── Header (fixed size: 4KB)
│   ├── Magic bytes: "TRAG"
│   ├── Version: u32
│   ├── Node count: u64
│   ├── Root offset: u64
│   └── Index table offset: u64
│
├── Index Table (node_id -> offset mapping)
│   ├── NodeId(1) -> offset 4096
│   ├── NodeId(2) -> offset 4512
│   └── ...
│
└── Node Data (MessagePack serialized)
    ├── Node 1 data
    ├── Node 2 data
    └── ...
```

### Access Pattern

```rust
impl MmapTree {
    pub fn get_node(&self, id: NodeId) -> Option<TreeNode> {
        // 1. Read offset from index table (already in memory via mmap)
        let offset = self.index_table.get(&id)?;
        
        // 2. Read node data from mmap (OS handles paging)
        let node_data = &self.mmap[*offset..];
        
        // 3. Deserialize just this node
        rmp_serde::from_slice(node_data).ok()
    }
}
```

---

## 3. Experience Grafting

**Problem**: Agent decisions should persist and inform future sessions.

**Solution**: Append-only log with structured extraction.

### Extraction Flow

```
SubagentStop Hook
      │
      ▼
Read agent transcript (JSONL)
      │
      ▼
Extract key patterns:
  - "I decided to..." → Decision
  - "The reason is..." → Rationale  
  - Files modified → files_touched
      │
      ▼
Append to experience.jsonl
      │
      ▼
Load into next session's anchor context
```

### Experience Entry

```json
{
  "timestamp": 1706940000,
  "agent_id": "agent-abc123",
  "session_id": "sess-xyz789",
  "decision": "Used singleton pattern for database connection",
  "rationale": "Ensures single connection pool across modules",
  "files_touched": ["src/db/connection.rs", "src/db/mod.rs"],
  "outcome": "success"
}
```

### Loading Strategy

```rust
impl ContextManager {
    fn load_relevant_experiences(&self, project: &Path) -> Vec<Experience> {
        let all = self.storage.load_experiences(project)?;
        
        // Filter and rank by relevance
        all.into_iter()
            .filter(|e| e.timestamp > recent_threshold)
            .sorted_by_key(|e| std::cmp::Reverse(e.timestamp))
            .take(10)  // Keep context manageable
            .collect()
    }
}
```

---

## 4. Hybrid Retrieval Routing

**Problem**: Different queries need different retrieval strategies.

**Solution**: Query classifier that routes to optimal index.

### Classification Rules

```rust
impl QueryClassifier {
    pub fn classify(&self, query: &str) -> QueryIntent {
        // Structural patterns (use tree)
        if query.contains("what calls") 
            || query.contains("who imports")
            || query.contains("dependencies of") {
            return QueryIntent::Structural;
        }
        
        // Positional patterns (use tree)
        if query.contains("in section")
            || query.contains("in file")
            || query.contains("in module") {
            return QueryIntent::Structural;
        }
        
        // Semantic patterns (use vector)
        if query.contains("how does")
            || query.contains("explain")
            || query.contains("similar to") {
            return QueryIntent::Semantic;
        }
        
        // Default to hybrid
        QueryIntent::Hybrid
    }
}
```

### Reciprocal Rank Fusion

```rust
fn merge_rrf(
    tree_results: Vec<(NodeId, f32)>,
    vector_results: Vec<(NodeId, f32)>,
    k: f32,  // Usually 60
) -> Vec<(NodeId, f32)> {
    let mut scores: HashMap<NodeId, f32> = HashMap::new();
    
    // Add tree scores
    for (i, (node_id, _)) in tree_results.iter().enumerate() {
        let rrf_score = 1.0 / (k + i as f32 + 1.0);
        *scores.entry(*node_id).or_default() += rrf_score;
    }
    
    // Add vector scores
    for (i, (node_id, _)) in vector_results.iter().enumerate() {
        let rrf_score = 1.0 / (k + i as f32 + 1.0);
        *scores.entry(*node_id).or_default() += rrf_score;
    }
    
    // Sort by combined score
    let mut merged: Vec<_> = scores.into_iter().collect();
    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    merged
}
```

---

## 5. Context Sandwich Construction

**Problem**: Sub-agents need structured context with clear boundaries.

**Solution**: Three-layer sandwich with explicit separation.

### Layer Definitions

```
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 1: ANCHOR (Immutable)                                      │
│ Purpose: Keep agent aligned with project goals                   │
├──────────────────────────────────────────────────────────────────┤
│ • Project rules (from .claude/rules.md)                          │
│ • Parent agent constraints ("Focus only on auth module")         │
│ • Recent team decisions (from experience pool)                   │
│ • Coding standards                                               │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 2: FOCUS (Mutable, agent can request expansion)            │
│ Purpose: Provide working context for the task                    │
├──────────────────────────────────────────────────────────────────┤
│ • Primary files for the task                                     │
│ • Auto-loaded dependencies (direct imports)                      │
│ • Type definitions                                               │
│ • Related test files                                             │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ LAYER 3: HORIZON (Read-only, low detail)                         │
│ Purpose: Peripheral vision for self-service expansion            │
├──────────────────────────────────────────────────────────────────┤
│ • Directory structure skeleton                                   │
│ • Module names and descriptions                                  │
│ • Public API signatures                                          │
│ • "For details, request expansion of [node_id]"                  │
└──────────────────────────────────────────────────────────────────┘
```

### Rendered Output Example

```markdown
# PROJECT CONTEXT

## Rules
- Always use async/await for I/O operations
- Prefer composition over inheritance
- All public APIs must have doc comments

## Recent Decisions
- agent-abc: Used singleton for DB connection (2 hours ago)
- agent-xyz: Chose JWT over sessions for auth (yesterday)

---

## Focus Area

### src/auth/handler.rs (primary)
```rust
pub async fn login(req: LoginRequest) -> Result<Token, AuthError> {
    // ... full content ...
}
```

### src/auth/token.rs (dependency)
```rust
pub struct Token { ... }
impl Token { ... }
```

---

## Project Structure (overview)
```
src/
├── auth/           # Authentication module [expand: node_123]
├── api/            # HTTP handlers [expand: node_124]
├── db/             # Database layer [expand: node_125]
└── utils/          # Shared utilities [expand: node_126]
```

To see details, mention "expand node_XXX".
```

---

## 6. Auto-Init Detection

**Problem**: New projects should be seamlessly initialized.

**Solution**: Detection on SessionStart with user-friendly prompts.

### Detection Flow

```
SessionStart
      │
      ▼
Check daemon running?
      │
      ├─ No → Start daemon (async), show message
      │
      └─ Yes → Check project initialized?
               │
               ├─ No → Show prompt: "Run /init-project"
               │       OR auto-init if config allows
               │
               └─ Yes → Load context (async)
                        └─ Show: "✓ Engram active"
```

### User Configuration

```yaml
# ~/.engram/config.yaml
auto_init:
  enabled: false          # Set to true for automatic init
  min_files: 10           # Don't auto-init tiny repos
  exclude_patterns:
    - "**/node_modules/**"
    - "**/vendor/**"
    - "**/.git/**"

# Per-project override
# .engram/config.yaml in project root
auto_init:
  enabled: true
```
