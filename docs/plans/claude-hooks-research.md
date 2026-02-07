# Claude Code Hook System Research (for Memory Sync Design)

Date: 2026-02-07

## Official Hook Model (Anthropic)

Claude Code hooks are configured in Claude settings under `"hooks"`, with event-specific matchers and commands. Hooks receive structured JSON on `stdin` and can return structured output for control/context behavior.

### Key Mechanics Relevant to Memory

1. **Event-driven lifecycle**
   - Core events include `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`, `PreCompact`, and `SessionEnd`.
   - Additional events now include `Notification`, `PreCompact`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `SubagentStop`, `TaskCompleted`, and `SessionStart` (event availability depends on workflow/config).

2. **Structured input contract**
   - Hook input includes common fields such as `session_id`, `transcript_path`, `cwd`, and `hook_event_name`.
   - Event-specific fields are provided for tool events and session lifecycle data (for example, `SessionStart` includes startup source; `SessionEnd` includes stop reason).

3. **Control path + context injection**
   - Hooks can emit JSON output to influence behavior for selected events (for example, block/continue decisions for pre-execution gates).
   - `UserPromptSubmit` supports adding context to Claude at prompt time.

4. **Async hook execution support**
   - Claude Code supports asynchronous hook execution patterns so heavy work can run without blocking interactive latency.

5. **Operational behavior**
   - Hook settings are loaded at startup from settings sources, and updates require explicit review/approval in Claude Code.

## Implications for TreeRAG

1. Treat hooks as **event ingest + context injection edges**, not as primary storage.
2. Keep hook scripts fast; push heavy work into daemon IPC endpoints.
3. Use `UserPromptSubmit` for deterministic memory retrieval/injection; use `PostToolUse`/`SubagentStop`/`TaskCompleted`/`SessionEnd` for memory writes.
4. Add handlers for newly supported events (`PostToolUseFailure`, `TaskCompleted`, `SessionEnd`) to prevent lifecycle blind spots.
5. Ensure hook output format is valid JSON where control/context behavior is needed.

## Design Constraints Derived from Official Docs

- Hook scripts must be resilient to missing optional fields and unknown tool payload variants.
- Memory writes should be idempotent where possible (same event may be retried).
- Lifecycle event-to-API mapping must be explicit to avoid unsupported action drift.

## Sources

- Claude Code Hooks reference: https://docs.anthropic.com/en/docs/claude-code/hooks
- Claude Code Hooks guide: https://docs.anthropic.com/en/docs/claude-code/hooks-guide
- Claude Code Settings (hooks config behavior): https://docs.anthropic.com/en/docs/claude-code/settings
