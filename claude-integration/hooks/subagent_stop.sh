#!/bin/bash
# TreeRAG Hook: SubagentStop
# Fires when a subagent completes
# Input: JSON with agent_id, outcome, files_touched

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Read subagent result from stdin
INPUT=$(cat)
AGENT_ID=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("agent_id","unknown"))' 2>/dev/null || echo "unknown")
DECISION=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("outcome","completed"))' 2>/dev/null || echo "completed")

# Build experience object
EXPERIENCE='{
    "agent_id":"'"$AGENT_ID"'",
    "decision":"'"$DECISION"'",
    "files_touched":[],
    "timestamp":'"$(date +%s)"'
}'

# Fire-and-forget: graft experience
treerag_send_async '{"action":"graft_experience","cwd":"'"$PWD"'","experience":'"$EXPERIENCE"'}'

exit 0
