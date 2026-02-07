#!/bin/bash
# Engram Slash Command: /scout-status
# Show Engram indexing status

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../hooks/common.sh"

echo "📊 Engram Status"
echo "================="

# Check daemon
if ! engram_is_running; then
    echo "❌ Daemon: Not running"
    echo ""
    echo "Start with: cargo run --bin engram-daemon"
    exit 0
fi

echo "✓ Daemon: Running"

# Get daemon status
RESULT=$(engram_send '{"action":"status"}' 2)

if [[ -n "$RESULT" ]]; then
    VERSION=$(echo "$RESULT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("data",{}).get("version","unknown"))' 2>/dev/null || echo "unknown")
    UPTIME=$(echo "$RESULT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("data",{}).get("uptime_secs",0))' 2>/dev/null || echo "0")
    PROJECTS=$(echo "$RESULT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("data",{}).get("projects_loaded",0))' 2>/dev/null || echo "0")
    MEMORY=$(echo "$RESULT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("data",{}).get("memory_usage_bytes",0))' 2>/dev/null || echo "0")
    
    echo "  Version: $VERSION"
    echo "  Uptime: ${UPTIME}s"
    echo "  Projects loaded: $PROJECTS"
    echo "  Memory: $((MEMORY / 1024 / 1024))MB"
fi

echo ""

# Check project initialization
INIT_RESULT=$(engram_send '{"action":"check_init","cwd":"'"$PWD"'"}' 1)

if [[ "$INIT_RESULT" == *'"initialized":true'* ]]; then
    echo "✓ Project: Initialized"
    echo "  Path: $PWD"
else
    echo "❌ Project: Not initialized"
    echo "  Run /init-project to enable smart context"
fi
