# TreeRAG Requirement Verification: Claude Lifecycle Memory + API

Date: 2026-02-07

## Requirement Under Review

Goal: Claude Code agents can access, retrieve, and update memory through Claude lifecycle hooks, with synchronized in-memory and file-backed memory, plus an API for memory interaction.

## Verification Method

- Static implementation audit of hooks, IPC protocol, daemon handlers, context manager, and storage.
- Focused review of lifecycle coverage and synchronization behavior.

## Current Capability Matrix

| Requirement | Status | Evidence |
|---|---|---|
| Retrieve context/memory during agent flow | Partial | `get_context` returns rendered context including recent experiences loaded from storage (`crates/treerag-ipc/src/protocol.rs`, `crates/treerag-context/src/manager.rs`, `crates/treerag-context/src/render.rs`) |
| Update memory from Claude lifecycle hooks | Partial | `subagent_stop.sh` sends `graft_experience`; daemon appends to experience log (`claude-integration/hooks/subagent_stop.sh`, `crates/treerag-daemon/src/handler.rs`, `crates/treerag-indexer/src/storage/mod.rs`) |
| File + in-memory sync for memory | Partial | New experiences are appended to file and injected into active scopes, but file-change notifications do not trigger reindex/sync and tree cache invalidation is missing (`crates/treerag-daemon/src/handler.rs`, `crates/treerag-context/src/manager.rs`) |
| API to interact with memory (read/query/update) | Fail | IPC has no explicit memory CRUD/query API; only `GraftExperience` (append) and indirect retrieval through `GetContext` (`crates/treerag-ipc/src/protocol.rs`) |
| Full lifecycle hook coverage for memory persistence | Fail | `session_end.sh` sends `session_end` action that is not defined in protocol; `pre_compact.sh` is a no-op (`claude-integration/hooks/session_end.sh`, `claude-integration/hooks/pre_compact.sh`, `crates/treerag-ipc/src/protocol.rs`) |

## Critical Gaps

1. Protocol/handler mismatch: `session_end` hook action is unsupported.
2. No first-class memory API for read/search/update/delete.
3. Hook cache flow is incomplete: `user_prompt.sh` reads `/tmp/treerag_cache/*.ctx`, but regular lifecycle does not persist refreshed cache.
4. `notify_file_change` path is acknowledged but does not mutate memory/index state.
5. Durability and consistency are weak for writes: `graft_experience` is async fire-and-forget from handler (ack before write completion).

## Verdict

Implementation is **not yet sufficient** to satisfy the requirement end-to-end.

What exists is a strong foundation (context retrieval, experience append, hook wiring), but the system currently lacks:
- full lifecycle memory synchronization,
- explicit memory interaction API,
- correctness guarantees for memory durability and freshness.

## Immediate Acceptance Criteria for “Requirement Met”

1. Claude lifecycle events can both read and write memory without unsupported actions.
2. API supports at least memory append + list/search + get-by-id + patch/delete semantics.
3. File and in-memory states converge deterministically after writes and file-change events.
4. Integration tests prove persistence and retrieval across daemon restarts and concurrent requests.
