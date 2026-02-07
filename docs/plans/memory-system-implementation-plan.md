# Engram Execution Plan: Claude Lifecycle Memory System

Date: 2026-02-07

## Objective

Deliver a production-ready memory system where Claude Code agents can:
- read memory during prompt/tool/subagent flow,
- write/update memory from lifecycle events,
- rely on synchronized in-memory and file-backed state,
- interact through explicit IPC memory APIs.

## Target Architecture

### Memory Data Model

Unify memory around a first-class `MemoryEntry` (instead of implicit append-only experience records):
- `id` (UUID)
- `project_hash`
- `kind` (`decision`, `tool_observation`, `failure`, `session_summary`, `task_result`, `context_note`)
- `content`
- `tags` (files, tool names, topic labels)
- `created_at`, `updated_at`
- `session_id`, `subagent_id` (optional)
- `deleted` (soft delete/tombstone)

### Sync Model

- **Write-through path**: IPC write -> daemon validates -> append to durable log -> apply to in-memory index -> ack.
- **Read path**: in-memory index first, fallback/rebuild from file on cache miss/restart.
- **Reconciliation**: on daemon startup and project load, replay memory log to rebuild in-memory index.

## API Plan (IPC)

Add request/response variants in `crates/engram-ipc/src/protocol.rs`:
- `MemoryPut { cwd, entry }`
- `MemoryGet { cwd, id }`
- `MemoryList { cwd, limit, before, kinds, tags }`
- `MemorySearch { cwd, query, limit, kinds, tags }`
- `MemoryPatch { cwd, id, patch }`
- `MemoryDelete { cwd, id }`
- `MemorySync { cwd }` (forces file->memory reconciliation)

Response variants:
- `MemoryEntry`, `MemoryEntries`, `MemoryAck`, `MemoryStats`.

Error model:
- explicit invalid payload, not found, sync conflict, storage unavailable.

## Hook-to-API Mapping

| Claude hook event | Engram action | Required result |
|---|---|---|
| `SessionStart` | `MemorySync` + `PrepareContext` | Fresh in-memory view before interaction |
| `UserPromptSubmit` | `MemorySearch` + `GetContext` | Deterministic context injection from memory |
| `PreToolUse` | Optional policy check (read memory constraints) | Allow/block with rationale |
| `PostToolUse` | `MemoryPut(kind=tool_observation)` + `NotifyFileChange` | Persist tool outcomes + file impacts |
| `PostToolUseFailure` | `MemoryPut(kind=failure)` | Capture failing commands/tools |
| `SubagentStop` | `MemoryPut(kind=decision/task_result)` | Persist subagent outcomes |
| `TaskCompleted` | `MemoryPut(kind=task_result)` | Persist final outcomes for retrieval |
| `PreCompact` | `MemoryPut(kind=session_summary)` | Preserve critical context before compaction |
| `SessionEnd` | `MemoryPut(kind=session_summary)` + flush | Ensure durable end-of-session memory |

## Phased Delivery Plan

## Phase 1: Protocol/Handler Alignment (Blocker Removal)

Scope:
- Fix unsupported lifecycle action mismatch (`session_end`).
- Add minimal memory read/write endpoints (`MemoryPut`, `MemoryList`, `MemoryGet`).
- Ensure daemon handler returns write result only after durable append.

Changes:
- `crates/engram-ipc/src/protocol.rs`
- `crates/engram-daemon/src/handler.rs`
- `claude-integration/hooks/session_end.sh`

Acceptance:
- No hook sends unknown IPC actions.
- Session end persists at least one summary memory entry.
- Integration test proves write durability before ACK.

## Phase 2: Storage + In-Memory Index

Scope:
- Implement `MemoryStore` service with:
  - append log file (for durability),
  - in-memory project index (for low-latency reads),
  - replay/rebuild at startup.
- Add patch/delete semantics with tombstones.

Changes:
- New module(s): `crates/engram-context/src/memory.rs` (or dedicated crate)
- Extend storage layer in `crates/engram-indexer/src/storage/`
- Wire in `ProjectManager` and daemon startup path.

Acceptance:
- Restart-safe read consistency (same entries before/after restart).
- Concurrent writes preserve order and no data loss.
- Memory search/list p99 latency target defined and measured.

## Phase 3: Hook Lifecycle Completion

Scope:
- Update hook scripts to cover full lifecycle mapping above.
- Add support for `PostToolUseFailure` and `TaskCompleted` hook handlers.
- Replace brittle cache-only retrieval with API-first retrieval + optional cache.

Changes:
- `claude-integration/hooks/*.sh`
- `claude-integration/settings.json`
- `claude-integration/commands/refresh-context.sh` (keep as manual override)

Acceptance:
- Hooks produce valid JSON/input handling for all configured events.
- `UserPromptSubmit` returns memory-backed context even on cold cache.
- Subagent and session boundaries always persist outcomes.

## Phase 4: Retrieval Quality + Ranking

Scope:
- Implement ranking for `MemorySearch`:
  - recency,
  - event kind priority,
  - file/path overlap,
  - lexical relevance to prompt.
- Feed selected memories into context renderer with provenance metadata.

Changes:
- `crates/engram-context/src/router.rs`
- `crates/engram-context/src/render.rs`
- Memory ranking module.

Acceptance:
- Retrieval tests show relevant memory selection across multi-session scenarios.
- Rendered context includes source memory IDs for debugging.

## Phase 5: Hardening + Observability

Scope:
- Add metrics for memory writes/reads, replay time, cache hit, failed writes.
- Add repair/recovery command for corrupted memory log segments.
- Add rate limits/size limits for hook-originated payloads.

Changes:
- `crates/engram-core/src/metrics.rs`
- CLI commands (for debug/repair): `crates/engram-cli/src/main.rs`

Acceptance:
- Operational dashboard/log lines can diagnose stale memory and sync lag.
- Recovery procedure validated in integration test.

## Test Plan (Required)

1. Unit tests
- Serialization/deserialization for all new IPC variants.
- Memory store append/replay/patch/delete logic.
- Ranking and filtering logic.

2. Integration tests
- Hook event -> IPC -> storage -> retrieval roundtrip.
- Concurrent `MemoryPut` from multiple simulated hooks.
- Restart consistency for in-memory rebuild.
- Session end/compact persistence behavior.

3. End-to-end tests
- Simulated Claude lifecycle script sequence (start -> prompt -> tool -> subagent -> compact -> end).
- Verify retrieved context on next prompt includes expected prior memories.

4. Performance tests
- p99 write latency for memory append.
- p99 read/search latency under 10k+ memory entries.

## Migration Strategy

1. Backward compatibility
- Keep `GraftExperience` temporarily; internally map to `MemoryPut(kind=decision)`.

2. Data migration
- Replay legacy `experience.jsonl` into new memory schema on first startup after upgrade.

3. Cutover
- Phase 1 deploy with dual-write (legacy + new memory store).
- Phase 2 switch reads to new API, keep fallback for one release.
- Phase 3 remove legacy code paths after successful bake period.

## Risks and Mitigations

1. Hook payload variance across Claude versions
- Mitigation: schema validation + tolerant parser + robust defaults.

2. Memory growth / disk bloat
- Mitigation: retention policy, compaction jobs, size caps per project.

3. Race conditions under concurrent hooks
- Mitigation: per-project async lock and ordered append queue.

4. Relevance drift (too much low-signal memory)
- Mitigation: ranking + kind-based weighting + expiry/archival.

## Definition of Done

The requirement is considered met when:
1. Full configured lifecycle events can read/write memory without protocol mismatches.
2. API supports explicit memory interaction (put/get/list/search/patch/delete).
3. Memory state is durable and synchronized between file and in-memory index.
4. E2E tests demonstrate cross-session memory retrieval quality and correctness.
