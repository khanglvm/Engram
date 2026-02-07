#!/bin/bash
# Engram Hook: PreCompact
# Fires before context compaction
# Opportunity to save important context before it's lost

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Currently a no-op - could save session state in future
# The daemon already maintains experience pool, so most
# important context is preserved automatically

exit 0
