#!/bin/bash
# Engram Hook: PostToolUse
# Fires after Claude modifies a file
# Input: JSON with tool_name and tool_input

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Read tool info from stdin
INPUT=$(cat)
TOOL=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || echo "")
FILE=$(echo "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}); print(ti.get("path","") or ti.get("file_path",""))' 2>/dev/null || echo "")

# Only care about file-modifying tools
case "$TOOL" in
    Write|Edit|Create|Delete|write_to_file|replace_file_content|multi_replace_file_content)
        if [[ -n "$FILE" ]]; then
            # Determine change type
            CHANGE_TYPE="modified"
            [[ "$TOOL" == "Create" || "$TOOL" == "write_to_file" ]] && CHANGE_TYPE="created"
            [[ "$TOOL" == "Delete" ]] && CHANGE_TYPE="deleted"
            
            # Fire-and-forget: notify file change
            ESCAPED_FILE=$(json_escape "$FILE")
            engram_send_async '{"action":"notify_file_change","cwd":"'"$PWD"'","path":'"$ESCAPED_FILE"',"change_type":"'"$CHANGE_TYPE"'"}'
        fi
        ;;
esac

exit 0
