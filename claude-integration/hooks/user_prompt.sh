#!/bin/bash
# TreeRAG Hook: UserPromptSubmit
# Fires before Claude processes user prompt
# Input: JSON with prompt field on stdin

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

treerag_ensure_cache

# Read prompt from stdin
INPUT=$(cat)
PROMPT=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("prompt",""))' 2>/dev/null || echo "")

PROJECT_HASH=$(treerag_project_hash)
CACHE_FILE="$TREERAG_CACHE_DIR/${PROJECT_HASH}.ctx"

# 1. Return cached context immediately (if available)
if [[ -f "$CACHE_FILE" ]]; then
    CONTEXT=$(cat "$CACHE_FILE")
    # Output as additionalContext for Claude
    ESCAPED=$(echo "$CONTEXT" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
    echo '{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":'"$ESCAPED"'}}'
fi

# 2. Fire-and-forget: prepare context for next prompt
if [[ -n "$PROMPT" ]] && treerag_is_running; then
    ESCAPED_PROMPT=$(json_escape "$PROMPT")
    treerag_send_async '{"action":"prepare_context","cwd":"'"$PWD"'","prompt":'"$ESCAPED_PROMPT"'}'
fi

exit 0
