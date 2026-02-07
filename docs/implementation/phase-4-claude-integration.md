# Phase 4: Claude Code Integration

> **Goal**: Integrate Engram with Claude Code via hooks and custom commands for seamless AI-assisted development.

## Overview

| Aspect | Detail |
|--------|--------|
| **Duration** | 1-2 weeks |
| **Priority** | High |
| **Dependencies** | Phase 3 (Context Management) |
| **Deliverables** | Hook scripts, slash commands, installer |

---

## 4.1 Hook Implementation

### Hook Mapping

| Hook Event | Purpose | Blocking | Script |
|------------|---------|----------|--------|
| SessionStart | Load project context | No (<5ms) | `session_start.sh` |
| UserPromptSubmit | Inject relevant context | No (<3ms) | `user_prompt.sh` |
| PreToolUse | Validate/expand scope | No (<2ms) | `pre_tool_use.sh` |
| PostToolUse | Notify file changes | No (<1ms) | `post_tool_use.sh` |
| SubagentStart | Inject context sandwich | No (<5ms) | `subagent_start.sh` |
| SubagentStop | Graft experience | No (<1ms) | `subagent_stop.sh` |
| PreCompact | Save important context | No (<5ms) | `pre_compact.sh` |
| SessionEnd | Persist session state | No (<3ms) | `session_end.sh` |

### Non-Blocking Pattern

All hooks follow this pattern to avoid blocking Claude Code:

```bash
#!/bin/bash
# Pattern: Fire-and-forget with cached response

SOCKET="/tmp/engram.sock"
CACHE_DIR="/tmp/engram_cache"

# 1. Return cached context immediately (if available)
CACHE_FILE="$CACHE_DIR/$(echo -n "$PWD" | md5).ctx"
if [[ -f "$CACHE_FILE" ]]; then
    cat "$CACHE_FILE"
fi

# 2. Fire-and-forget: request fresh context for NEXT call
(echo '{"action":"prepare","cwd":"'"$PWD"'"}' | nc -U -w0 "$SOCKET") &

exit 0
```

---

## 4.2 Hook Scripts

### session_start.sh

```bash
#!/bin/bash
# Fires when Claude Code session begins

SOCKET="/tmp/engram.sock"

# Check if daemon is running
if ! nc -z -U "$SOCKET" 2>/dev/null; then
    echo "⚠️ Engram daemon not running. Start with: engram start"
    exit 0
fi

# Check if project is initialized
RESULT=$(echo '{"action":"check_init","cwd":"'"$PWD"'"}' | nc -U -w1 "$SOCKET" 2>/dev/null)

if [[ "$RESULT" == *'"initialized":false'* ]]; then
    echo "📋 Project not indexed by Engram."
    echo "   Run /init-project to enable smart context."
    exit 0
fi

# Fire-and-forget: prepare session context
(echo '{"action":"prepare_session","cwd":"'"$PWD"'"}' | nc -U -w0 "$SOCKET") &

echo "✓ Engram context loaded"
exit 0
```

### user_prompt.sh

```bash
#!/bin/bash
# Fires before Claude processes user prompt
# Input: JSON with prompt field

SOCKET="/tmp/engram.sock"
CACHE_DIR="/tmp/engram_cache"
PROJECT_HASH=$(echo -n "$PWD" | md5)

# Read prompt from stdin
read -r INPUT
PROMPT=$(echo "$INPUT" | jq -r '.prompt // empty')

# 1. Return cached context immediately
CACHE_FILE="$CACHE_DIR/${PROJECT_HASH}.ctx"
if [[ -f "$CACHE_FILE" ]]; then
    # Output as additionalContext for Claude
    CONTEXT=$(cat "$CACHE_FILE")
    echo '{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"'"$(echo "$CONTEXT" | jq -Rs .)"'"}}'
fi

# 2. Fire-and-forget: prepare context for next prompt
if [[ -n "$PROMPT" ]]; then
    (echo '{"action":"prepare_context","cwd":"'"$PWD"'","prompt":"'"$(echo "$PROMPT" | jq -Rs .)"'"}' | nc -U -w0 "$SOCKET") &
fi

exit 0
```

### post_tool_use.sh

```bash
#!/bin/bash
# Fires after Claude modifies a file

SOCKET="/tmp/engram.sock"

# Read tool info from stdin
read -r INPUT
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')
FILE=$(echo "$INPUT" | jq -r '.tool_input.path // .tool_input.file_path // empty')

# Only care about file-modifying tools
case "$TOOL" in
    Write|Edit|Create|Delete)
        if [[ -n "$FILE" ]]; then
            # Fire-and-forget: notify file change
            (echo '{"action":"notify_file_change","cwd":"'"$PWD"'","path":"'"$FILE"'","change_type":"modified"}' | nc -U -w0 "$SOCKET") &
        fi
        ;;
esac

exit 0
```

### subagent_start.sh

```bash
#!/bin/bash
# Fires when a subagent is spawned

SOCKET="/tmp/engram.sock"

# Read subagent info
read -r INPUT
AGENT_ID=$(echo "$INPUT" | jq -r '.agent_id // empty')
AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // empty')

# Request context sandwich for subagent
RESULT=$(echo '{"action":"get_sandwich","cwd":"'"$PWD"'","agent_id":"'"$AGENT_ID"'","agent_type":"'"$AGENT_TYPE"'"}' | nc -U -w1 "$SOCKET" 2>/dev/null)

if [[ -n "$RESULT" ]]; then
    CONTEXT=$(echo "$RESULT" | jq -r '.context // empty')
    if [[ -n "$CONTEXT" ]]; then
        echo '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"'"$(echo "$CONTEXT" | jq -Rs .)"'"}}'
    fi
fi

exit 0
```

### subagent_stop.sh

```bash
#!/bin/bash
# Fires when a subagent completes

SOCKET="/tmp/engram.sock"

# Read subagent result
read -r INPUT
AGENT_ID=$(echo "$INPUT" | jq -r '.agent_id // empty')
TRANSCRIPT=$(echo "$INPUT" | jq -r '.agent_transcript_path // empty')

# Fire-and-forget: graft experience
(echo '{"action":"graft_experience","cwd":"'"$PWD"'","agent_id":"'"$AGENT_ID"'","transcript_path":"'"$TRANSCRIPT"'"}' | nc -U -w0 "$SOCKET") &

exit 0
```

---

## 4.3 Slash Commands

### /init-project

```markdown
<!-- .claude/commands/init-project.md -->
Initialize the current project for Engram smart context.

This command will:
1. Scan the codebase structure (fast, ~30 seconds)
2. Parse code for symbols and dependencies
3. Start background AI enrichment

Usage: /init-project [--quick]

Options:
  --quick  Skip AI enrichment (faster, less detailed)
```

Implementation script:

```bash
#!/bin/bash
# .claude/commands/init-project.sh

SOCKET="/tmp/engram.sock"

echo "🔍 Initializing Engram for: $PWD"

# Check daemon
if ! nc -z -U "$SOCKET" 2>/dev/null; then
    echo "Starting Engram daemon..."
    engram start
    sleep 1
fi

# Parse args
QUICK=false
[[ "$*" == *"--quick"* ]] && QUICK=true

# Send init request
if $QUICK; then
    RESULT=$(echo '{"action":"init_project","cwd":"'"$PWD"'","async_mode":false}' | nc -U -w60 "$SOCKET")
else
    RESULT=$(echo '{"action":"init_project","cwd":"'"$PWD"'","async_mode":true}' | nc -U -w60 "$SOCKET")
fi

# Parse result
if [[ "$RESULT" == *'"status":"ok"'* ]]; then
    echo "✓ Project initialized successfully"
    
    # Show summary
    FILES=$(echo "$RESULT" | jq -r '.data.total_files // "unknown"')
    MODULES=$(echo "$RESULT" | jq -r '.data.modules // "unknown"')
    
    echo ""
    echo "Summary:"
    echo "  Files: $FILES"
    echo "  Modules: $MODULES"
    
    if ! $QUICK; then
        echo ""
        echo "AI enrichment running in background..."
        echo "Check status with /scout-status"
    fi
else
    echo "✗ Initialization failed"
    echo "$RESULT" | jq -r '.message // empty'
fi
```

### /scout-status

```markdown
<!-- .claude/commands/scout-status.md -->
Show Engram indexing status for the current project.

Usage: /scout-status
```

### /refresh-context

```markdown
<!-- .claude/commands/refresh-context.md -->
Force refresh Engram context for the current session.

Usage: /refresh-context
```

---

## 4.4 Claude Settings Configuration

```json
// .claude/settings.json
{
  "hooks": {
    "SessionStart": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/session_start.sh"
      }
    ],
    "UserPromptSubmit": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/user_prompt.sh"
      }
    ],
    "PostToolUse": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/post_tool_use.sh",
        "matcher": ["Write", "Edit", "Create", "Delete"]
      }
    ],
    "SubagentStart": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/subagent_start.sh"
      }
    ],
    "SubagentStop": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/subagent_stop.sh"
      }
    ],
    "PreCompact": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/pre_compact.sh"
      }
    ],
    "SessionEnd": [
      {
        "type": "command",
        "command": "$PROJECT/.claude/hooks/session_end.sh"
      }
    ]
  }
}
```

---

## 4.5 Installer Script

```bash
#!/bin/bash
# claude-integration/install.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$HOME/.claude"

echo "Installing Engram Claude Code integration..."

# 1. Ensure .claude directory exists
mkdir -p "$CLAUDE_DIR/hooks"
mkdir -p "$CLAUDE_DIR/commands"

# 2. Copy hook scripts
cp "$SCRIPT_DIR/hooks/"*.sh "$CLAUDE_DIR/hooks/"
chmod +x "$CLAUDE_DIR/hooks/"*.sh

# 3. Copy command files
cp "$SCRIPT_DIR/commands/"*.md "$CLAUDE_DIR/commands/"
cp "$SCRIPT_DIR/commands/"*.sh "$CLAUDE_DIR/commands/"
chmod +x "$CLAUDE_DIR/commands/"*.sh 2>/dev/null || true

# 4. Merge settings
if [[ -f "$CLAUDE_DIR/settings.json" ]]; then
    echo "Merging with existing settings..."
    # Use jq to merge (simplified)
    cp "$CLAUDE_DIR/settings.json" "$CLAUDE_DIR/settings.json.bak"
fi
cp "$SCRIPT_DIR/settings.json" "$CLAUDE_DIR/settings.json"

# 5. Install launchd plist for daemon
PLIST="$HOME/Library/LaunchAgents/com.engram.daemon.plist"
cp "$SCRIPT_DIR/com.engram.daemon.plist" "$PLIST"

echo ""
echo "✓ Installation complete!"
echo ""
echo "Next steps:"
echo "  1. Start the daemon: engram start"
echo "  2. Initialize a project: cd /your/project && engram init"
echo "  3. Use Claude Code normally - context will be injected automatically"
```

---

## Testing Requirements

> Reference: [Testing Strategy](./testing-strategy.md) for general guidelines.
> Reference: [Benchmarks Reference](./benchmarks-reference.md) for performance targets.

### Unit Test Coverage

#### Hook Scripts

| Component | Required Tests |
|-----------|----------------|
| session_start.sh | Daemon running/not running; initialized/not initialized; output format |
| user_prompt.sh | Cache hit; cache miss; JSON parsing; context injection format |
| post_tool_use.sh | Each tool type; file path extraction; fire-and-forget timing |
| subagent_start.sh | Context sandwich retrieval; JSON output format |
| subagent_stop.sh | Experience grafting; transcript path handling |

**Edge cases to cover:**
- Daemon socket doesn't exist
- Daemon socket exists but daemon crashed
- Malformed JSON input to hook
- Very long file paths
- File paths with spaces and special characters
- Empty stdin input
- Concurrent hook invocations
- Hook timeout handling
- Cache directory doesn't exist
- Cache file corrupted

#### Slash Commands

| Component | Required Tests |
|-----------|----------------|
| init-project | Fresh project; already initialized; --quick flag; daemon not running |
| scout-status | Running; idle; error states |
| refresh-context | Success; failure; timeout |

**Edge cases to cover:**
- Project with no .gitignore
- Very large project (>100k files)
- Read-only project directory
- Disk space full during init
- Init interrupted (Ctrl+C)
- Concurrent init from multiple terminals

### Integration Test Coverage

#### Hook → Daemon Round-Trip

```
tests/
├── integration_hooks.rs
```

Tests must verify:
- session_start → daemon registers session
- user_prompt → context cache populated
- post_tool_use → file change notification received
- subagent_start → sandwich context returned
- subagent_stop → experience appended to log
- Full hook chain: start → prompt → tool → end

#### Installer

```
tests/
├── test_installer.sh
```

Tests must verify:
- Fresh install creates directories
- Upgrade preserves existing settings
- Rollback restores backup
- All scripts are executable
- launchd plist is valid XML
- Settings JSON is valid

### Performance Test Requirements

> **Critical**: Hook latency directly impacts Claude Code UX. All hooks must be non-blocking.

| Metric | Target | Source/Rationale |
|--------|--------|------------------|
| session_start.sh (total) | <5ms | Shell startup + socket check |
| user_prompt.sh (cache hit) | <2ms | File read + stdout |
| user_prompt.sh (cache miss) | <3ms | Fire-and-forget + minimal output |
| post_tool_use.sh | <1ms | Fire-and-forget only |
| subagent_start.sh (sync) | <10ms | Must wait for context |
| subagent_stop.sh | <1ms | Fire-and-forget only |
| Cache file read | <500µs | Small file on SSD |
| Socket connect + send | <200µs | Unix domain socket |

**Performance test structure:**
```bash
#!/bin/bash
# test_hook_latency.sh

# Warm up
for i in {1..10}; do
    .claude/hooks/session_start.sh > /dev/null 2>&1
done

# Measure 100 iterations
TIMES=()
for i in {1..100}; do
    START=$(date +%s%N)
    .claude/hooks/session_start.sh > /dev/null 2>&1
    END=$(date +%s%N)
    TIMES+=($((($END - $START) / 1000000)))  # Convert to ms
done

# Calculate P99
sorted=($(printf '%s\n' "${TIMES[@]}" | sort -n))
p99_index=$((${#sorted[@]} * 99 / 100))
echo "P99 latency: ${sorted[$p99_index]}ms"
```

### Resource Test Requirements

| Resource | Limit | Test |
|----------|-------|------|
| Hook memory (peak) | <5MB | Each hook invocation |
| Cache directory size | <10MB | All cached contexts |
| Hook file descriptors | 0 leaked | After 1000 invocations |
| Background processes | 0 orphaned | After hook exits |

**Resource test structure:**
```bash
#!/bin/bash
# test_hook_resources.sh

# Count open files before
FD_BEFORE=$(ls /proc/$$/fd 2>/dev/null | wc -l)

# Run hooks many times
for i in {1..1000}; do
    echo '{"prompt":"test"}' | .claude/hooks/user_prompt.sh > /dev/null 2>&1
done

# Wait for background processes
sleep 2

# Count open files after
FD_AFTER=$(ls /proc/$$/fd 2>/dev/null | wc -l)

# Check for orphan processes
ORPHANS=$(pgrep -f "engram" | wc -l)

echo "FD leak: $((FD_AFTER - FD_BEFORE))"
echo "Orphan processes: $ORPHANS"
```

### Error Recovery Testing

| Error Scenario | Expected Recovery |
|----------------|-------------------|
| Daemon not running | Display warning, exit 0 |
| Socket connection refused | Display warning, exit 0 |
| Socket timeout | Continue with cached context |
| Malformed daemon response | Ignore, exit 0 |
| Cache file permission denied | Skip cache, continue |
| jq parse error | Default to empty context |

### Compatibility Testing

Test on:
- macOS 12+ (Monterey and later)
- bash 3.2+ (macOS default)
- zsh 5.8+ (macOS default shell)
- Claude Code version compatibility

### Test Execution Commands

```bash
# Hook script tests (bash)
bats tests/hooks/*.bats

# Latency tests
./tests/test_hook_latency.sh

# Resource tests
./tests/test_hook_resources.sh

# Installer tests
./tests/test_installer.sh

# Integration with daemon
cargo test --test integration_hooks

# Full end-to-end (requires Claude Code)
# Manual: Follow verification steps below
```

### Manual Verification

1. Install integration: `./claude-integration/install.sh`
2. Start daemon: `engram start`
3. Initialize project: `cd /path/to/project && engram init`
4. Open Claude Code in the project
5. Verify session start message appears
6. Make a code change and verify indexing updates
7. Create a subagent task and verify context injection
8. Check experience is grafted on agent completion

---

## Deliverables Checklist

### Implementation
- [x] Hook scripts (9 total): common, session_start, user_prompt, pre_tool_use, post_tool_use, subagent_start, subagent_stop, pre_compact, session_end
- [x] Slash commands: init-project, scout-status, refresh-context (3 .md + 3 .sh)
- [x] Claude settings.json configuration
- [x] Installer script with backup support
- [x] launchd plist for daemon auto-start (in `integration/`)
- [x] Uninstaller script
- [ ] Documentation for manual setup

### Testing
- [ ] Unit tests for each hook script (bats)
- [ ] Unit tests for slash commands
- [ ] Integration tests for hook → daemon flow
- [ ] Installer tests (fresh + upgrade)
- [ ] Performance tests (all hooks <5ms P99)
- [ ] Resource tests (no leaks)
- [ ] Error recovery tests
- [ ] Compatibility tests (bash/zsh, macOS versions)
- [ ] Manual end-to-end verification

