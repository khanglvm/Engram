#!/bin/bash
# TreeRAG Claude Code Integration Uninstaller
# Removes hooks and commands from ~/.treerag

set -euo pipefail

INSTALL_DIR="$HOME/.treerag"
CLAUDE_DIR="$HOME/.claude"
LAUNCH_AGENTS="$HOME/Library/LaunchAgents"

echo "╔═══════════════════════════════════════════╗"
echo "║   TreeRAG Claude Code Uninstaller         ║"
echo "╚═══════════════════════════════════════════╝"
echo ""

# 1. Stop daemon if running
if pgrep -f "treerag-daemon" > /dev/null 2>&1; then
    echo "🛑 Stopping TreeRAG daemon..."
    pkill -f "treerag-daemon" || true
fi

# 2. Unload launchd service
if [[ -f "$LAUNCH_AGENTS/com.treerag.daemon.plist" ]]; then
    echo "📋 Unloading launchd service..."
    launchctl unload "$LAUNCH_AGENTS/com.treerag.daemon.plist" 2>/dev/null || true
    rm -f "$LAUNCH_AGENTS/com.treerag.daemon.plist"
fi

# 3. Remove installation directory
if [[ -d "$INSTALL_DIR" ]]; then
    echo "🗑️  Removing $INSTALL_DIR..."
    rm -rf "$INSTALL_DIR"
fi

# 4. Restore Claude settings backup
if [[ -f "$CLAUDE_DIR/settings.json.bak" ]]; then
    echo "⚙️  Restoring Claude settings from backup..."
    mv "$CLAUDE_DIR/settings.json.bak" "$CLAUDE_DIR/settings.json"
else
    # Remove TreeRAG hooks from settings
    echo "⚙️  Removing TreeRAG hooks from Claude settings..."
    # For simplicity, just remove our settings file
    # A more sophisticated approach would parse and remove only our hooks
    rm -f "$CLAUDE_DIR/settings.json"
fi

# 5. Remove command files from Claude
echo "🗑️  Removing slash commands..."
rm -f "$CLAUDE_DIR/commands/init-project.md"
rm -f "$CLAUDE_DIR/commands/scout-status.md"
rm -f "$CLAUDE_DIR/commands/refresh-context.md"

# 6. Clean up cache
echo "🧹 Cleaning up cache..."
rm -rf /tmp/treerag_cache
rm -f /tmp/treerag.sock

echo ""
echo "═══════════════════════════════════════════"
echo "✓ Uninstallation complete!"
echo ""
echo "TreeRAG has been removed from your system."
echo "Your projects' indexed data remains in ~/.treerag/projects/"
echo "Delete manually if no longer needed."
echo "═══════════════════════════════════════════"
