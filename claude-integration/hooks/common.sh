#!/bin/bash
# Engram Claude Code Integration - Common Utilities
# Source this file in hooks for shared functionality

ENGRAM_SOCKET="${ENGRAM_SOCKET:-/tmp/engram.sock}"
ENGRAM_CACHE_DIR="${ENGRAM_CACHE_DIR:-/tmp/engram_cache}"

# Check if daemon is running
engram_is_running() {
    [[ -S "$ENGRAM_SOCKET" ]] && nc -z -U "$ENGRAM_SOCKET" 2>/dev/null
}

# Send request to daemon (blocking, with timeout)
engram_send() {
    local request="$1"
    local timeout="${2:-1}"
    
    if ! engram_is_running; then
        return 1
    fi
    
    echo "$request" | nc -U -w"$timeout" "$ENGRAM_SOCKET" 2>/dev/null
}

# Send request to daemon (fire-and-forget, non-blocking)
engram_send_async() {
    local request="$1"
    
    if engram_is_running; then
        (echo "$request" | nc -U -w0 "$ENGRAM_SOCKET" 2>/dev/null) &
    fi
}

# Get project hash for cache key
engram_project_hash() {
    echo -n "$PWD" | md5 | cut -c1-16
}

# Ensure cache directory exists
engram_ensure_cache() {
    mkdir -p "$ENGRAM_CACHE_DIR" 2>/dev/null
}

# JSON escape a string
json_escape() {
    local str="$1"
    printf '%s' "$str" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}
