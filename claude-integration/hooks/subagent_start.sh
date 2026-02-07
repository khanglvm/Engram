#!/bin/bash
# TreeRAG Hook: SubagentStart
# Fires when a subagent is spawned
# Input: JSON with agent_id and agent_type

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Read subagent info from stdin
INPUT=$(cat)
AGENT_ID=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("agent_id",""))' 2>/dev/null || echo "")

if ! treerag_is_running; then
    exit 0
fi

# Request context for subagent (blocking - we need the response)
RESULT=$(treerag_send '{"action":"get_context","cwd":"'"$PWD"'","prompt":null}' 2)

if [[ -n "$RESULT" ]]; then
    CONTEXT=$(echo "$RESULT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("data",{}).get("context",""))' 2>/dev/null || echo "")
    if [[ -n "$CONTEXT" ]]; then
        ESCAPED=$(echo "$CONTEXT" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
        echo '{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":'"$ESCAPED"'}}'
    fi
fi

exit 0
