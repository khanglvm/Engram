#!/bin/bash
# Engram Hook: PreToolUse
# Fires before Claude uses a tool
# Input: JSON with tool_name and tool_input

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Read tool info from stdin
INPUT=$(cat)
TOOL=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || echo "")

# For file-reading tools, we could expand context scope
# Currently a no-op placeholder for future expansion
case "$TOOL" in
    Read|View|Search)
        # Future: Could notify daemon about files being accessed
        # for smarter context building
        ;;
esac

exit 0
