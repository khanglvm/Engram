#!/bin/bash
# Engram Slash Command: /refresh-context
# Force refresh context for current session

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../hooks/common.sh"

engram_ensure_cache

# Clear cached context
PROJECT_HASH=$(engram_project_hash)
CACHE_FILE="$ENGRAM_CACHE_DIR/${PROJECT_HASH}.ctx"
rm -f "$CACHE_FILE" 2>/dev/null

echo "🔄 Refreshing Engram context..."

if ! engram_is_running; then
    echo "⚠️ Daemon not running. Start with: cargo run --bin engram-daemon"
    exit 1
fi

# Request fresh context
RESULT=$(engram_send '{"action":"get_context","cwd":"'"$PWD"'","prompt":null}' 5)

if [[ -n "$RESULT" ]]; then
    CONTEXT=$(echo "$RESULT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("data",{}).get("context",""))' 2>/dev/null || echo "")
    
    if [[ -n "$CONTEXT" ]]; then
        # Save to cache
        echo "$CONTEXT" > "$CACHE_FILE"
        echo "✓ Context refreshed and cached"
        echo ""
        echo "Context preview:"
        echo "$CONTEXT" | head -10
        echo "..."
    else
        echo "✗ Failed to get context"
    fi
else
    echo "✗ No response from daemon"
fi
