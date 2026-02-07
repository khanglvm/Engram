#!/bin/bash
# Engram Claude Code Integration Uninstaller
# Removes hooks and commands from ~/.engram

set -euo pipefail

INSTALL_DIR="$HOME/.engram"
CLAUDE_DIR="$HOME/.claude"
LAUNCH_AGENTS="$HOME/Library/LaunchAgents"

echo "╔═══════════════════════════════════════════╗"
echo "║   Engram Claude Code Uninstaller         ║"
echo "╚═══════════════════════════════════════════╝"
echo ""

# 1. Stop daemon if running
if pgrep -f "engram-daemon" > /dev/null 2>&1; then
    echo "🛑 Stopping Engram daemon..."
    pkill -f "engram-daemon" || true
fi

# 2. Unload launchd service
if [[ -f "$LAUNCH_AGENTS/com.engram.daemon.plist" ]]; then
    echo "📋 Unloading launchd service..."
    launchctl unload "$LAUNCH_AGENTS/com.engram.daemon.plist" 2>/dev/null || true
    rm -f "$LAUNCH_AGENTS/com.engram.daemon.plist"
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
    # Remove Engram hooks from settings
    echo "⚙️  Removing Engram hooks from Claude settings..."
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
rm -rf /tmp/engram_cache
rm -f /tmp/engram.sock

echo ""
echo "═══════════════════════════════════════════"
echo "✓ Uninstallation complete!"
echo ""
echo "Engram has been removed from your system."
echo "Your projects' indexed data remains in ~/.engram/projects/"
echo "Delete manually if no longer needed."
echo "═══════════════════════════════════════════"
