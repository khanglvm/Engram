#!/bin/bash
# Engram Slash Command: /init-project
# Initialize the current project for Engram smart context

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../hooks/common.sh"

echo "🔍 Initializing Engram for: $PWD"

# Check daemon
if ! engram_is_running; then
    echo "⚠️ Engram daemon not running."
    echo "   Start with: cargo run --bin engram-daemon"
    exit 1
fi

# Parse args
QUICK=false
[[ "${*:-}" == *"--quick"* ]] && QUICK=true

# Send init request
if $QUICK; then
    RESULT=$(engram_send '{"action":"init_project","cwd":"'"$PWD"'","async_mode":false}' 60)
else
    RESULT=$(engram_send '{"action":"init_project","cwd":"'"$PWD"'","async_mode":true}' 60)
fi

# Parse result
if [[ "$RESULT" == *'"status":"ok"'* ]] || [[ "$RESULT" == *'"Ok"'* ]]; then
    echo "✓ Project initialized successfully"
    
    if ! $QUICK; then
        echo ""
        echo "Background indexing will continue..."
        echo "Check status with /scout-status"
    fi
else
    echo "✗ Initialization failed"
    echo "$RESULT"
fi
