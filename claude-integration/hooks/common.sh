#!/bin/bash
# TreeRAG Claude Code Integration - Common Utilities
# Source this file in hooks for shared functionality

TREERAG_SOCKET="${TREERAG_SOCKET:-/tmp/treerag.sock}"
TREERAG_CACHE_DIR="${TREERAG_CACHE_DIR:-/tmp/treerag_cache}"

# Check if daemon is running
treerag_is_running() {
    [[ -S "$TREERAG_SOCKET" ]] && nc -z -U "$TREERAG_SOCKET" 2>/dev/null
}

# Send request to daemon (blocking, with timeout)
treerag_send() {
    local request="$1"
    local timeout="${2:-1}"
    
    if ! treerag_is_running; then
        return 1
    fi
    
    echo "$request" | nc -U -w"$timeout" "$TREERAG_SOCKET" 2>/dev/null
}

# Send request to daemon (fire-and-forget, non-blocking)
treerag_send_async() {
    local request="$1"
    
    if treerag_is_running; then
        (echo "$request" | nc -U -w0 "$TREERAG_SOCKET" 2>/dev/null) &
    fi
}

# Get project hash for cache key
treerag_project_hash() {
    echo -n "$PWD" | md5 | cut -c1-16
}

# Ensure cache directory exists
treerag_ensure_cache() {
    mkdir -p "$TREERAG_CACHE_DIR" 2>/dev/null
}

# JSON escape a string
json_escape() {
    local str="$1"
    printf '%s' "$str" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}
