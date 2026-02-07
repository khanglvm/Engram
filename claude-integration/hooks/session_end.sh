#!/bin/bash
# Engram Hook: SessionEnd
# Fires when Claude Code session ends

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Read event payload from stdin (best-effort; tolerate malformed input)
INPUT=$(cat 2>/dev/null || true)

SESSION_ID=$(
    printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    data = json.load(sys.stdin)
except Exception:
    data = {}
session_id = data.get("session_id", "unknown")
print(session_id if isinstance(session_id, str) and session_id else "unknown")
' 2>/dev/null || echo "unknown"
)

STOP_REASON=$(
    printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    data = json.load(sys.stdin)
except Exception:
    data = {}
reason = data.get("stop_reason") or data.get("reason") or "ended"
print(reason if isinstance(reason, str) and reason else "ended")
' 2>/dev/null || echo "ended"
)

TRANSCRIPT_PATH=$(
    printf '%s' "$INPUT" | python3 -c 'import json,sys
try:
    data = json.load(sys.stdin)
except Exception:
    data = {}
path = data.get("transcript_path", "")
print(path if isinstance(path, str) else "")
' 2>/dev/null || echo ""
)

TIMESTAMP="$(date +%s)"
ENTRY_ID="session-end:${TIMESTAMP}"
if [[ "$SESSION_ID" != "unknown" ]]; then
    ENTRY_ID="session:${SESSION_ID}:${TIMESTAMP}"
fi

CONTENT="session_end:${STOP_REASON}"
if [[ -n "$TRANSCRIPT_PATH" ]]; then
    CONTENT="${CONTENT} transcript:${TRANSCRIPT_PATH}"
fi

ESCAPED_ENTRY_ID=$(json_escape "$ENTRY_ID")
ESCAPED_CONTENT=$(json_escape "$CONTENT")

SESSION_ID_VALUE="null"
if [[ "$SESSION_ID" != "unknown" ]]; then
    SESSION_ID_VALUE=$(json_escape "$SESSION_ID")
fi

ENTRY='{"id":'"$ESCAPED_ENTRY_ID"',"kind":"session_summary","content":'"$ESCAPED_CONTENT"',"tags":["hook","session_end"],"created_at":'"$TIMESTAMP"',"updated_at":'"$TIMESTAMP"',"session_id":'"$SESSION_ID_VALUE"',"subagent_id":null,"deleted":false}'

# Fire-and-forget: persist end-of-session summary as memory entry
engram_send_async '{"action":"memory_put","cwd":"'"$PWD"'","entry":'"$ENTRY"'}'

exit 0
