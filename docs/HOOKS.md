# TreeRAG Claude Code Hooks

## Overview

TreeRAG integrates with Claude Code via hooks that inject context and track agent decisions.

## Hook Events

| Hook | Timing | Purpose |
|------|--------|---------|
| `SessionStart` | Session begins | Load project context |
| `UserPromptSubmit` | Before prompt processing | Inject relevant context |
| `PreToolUse` | Before tool execution | Expand scope if needed |
| `PostToolUse` | After tool execution | Notify file changes |
| `SubagentStart` | Subagent spawned | Inject context sandwich |
| `SubagentStop` | Subagent completes | Graft experience |
| `PreCompact` | Before compaction | Save important context |
| `SessionEnd` | Session ends | Persist session summary memory |

## Non-Blocking Pattern

All hooks follow a non-blocking pattern:

1. **Return cached context immediately** (if available)
2. **Fire-and-forget** async request to daemon for next call
3. **Always exit 0** - never block Claude Code

```bash
# Example: user_prompt.sh pattern
# 1. Check cache -> return immediately
# 2. Background request to prepare fresh context
# 3. Exit 0
```

## Slash Commands

### /init-project

Initialize current project for TreeRAG indexing.

```
Usage: /init-project [--quick]

Options:
  --quick  Skip AI enrichment (faster)
```

### /scout-status

Show daemon and project status.

```
Usage: /scout-status
```

### /refresh-context

Force refresh cached context.

```
Usage: /refresh-context
```

## Installation

```bash
./claude-integration/install.sh
```

This installs:
- Hook scripts to `~/.treerag/hooks/`
- Command files to `~/.claude/commands/`
- Settings to `~/.claude/settings.json`

## Configuration

Hooks are configured in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [{
      "type": "command",
      "command": "$HOME/.treerag/hooks/session_start.sh"
    }],
    ...
  }
}
```
