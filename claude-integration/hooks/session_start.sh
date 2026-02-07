#!/bin/bash
# TreeRAG Hook: SessionStart
# Fires when Claude Code session begins

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check if daemon is running
if ! treerag_is_running; then
    echo "⚠️ TreeRAG daemon not running. Start with: treerag start"
    exit 0
fi

# Check if project is initialized
RESULT=$(treerag_send '{"action":"check_init","cwd":"'"$PWD"'"}' 1)

if [[ "$RESULT" == *'"initialized":false'* ]]; then
    echo "📋 Project not indexed by TreeRAG."
    echo "   Run /init-project to enable smart context."
    exit 0
fi

# Fire-and-forget: prepare session context
treerag_send_async '{"action":"prepare_context","cwd":"'"$PWD"'","prompt":null}'

echo "✓ TreeRAG context loaded"
exit 0
