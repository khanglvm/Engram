#!/bin/bash
# TreeRAG Claude Code Integration Installer
# Installs hooks and commands to ~/.treerag

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="$HOME/.treerag"
CLAUDE_DIR="$HOME/.claude"

echo "╔═══════════════════════════════════════════╗"
echo "║   TreeRAG Claude Code Integration Setup   ║"
echo "╚═══════════════════════════════════════════╝"
echo ""

# 1. Create installation directories
echo "📁 Creating directories..."
mkdir -p "$INSTALL_DIR/hooks"
mkdir -p "$INSTALL_DIR/commands"
mkdir -p "$CLAUDE_DIR/commands"

# 2. Copy hook scripts
echo "📋 Installing hook scripts..."
cp "$SCRIPT_DIR/hooks/"*.sh "$INSTALL_DIR/hooks/"
chmod +x "$INSTALL_DIR/hooks/"*.sh

# 3. Copy command files
echo "📋 Installing slash commands..."
cp "$SCRIPT_DIR/commands/"*.sh "$INSTALL_DIR/commands/"
cp "$SCRIPT_DIR/commands/"*.md "$INSTALL_DIR/commands/"
chmod +x "$INSTALL_DIR/commands/"*.sh

# Also copy to Claude commands directory
cp "$SCRIPT_DIR/commands/"*.md "$CLAUDE_DIR/commands/"

# 4. Handle Claude settings
echo "⚙️  Configuring Claude Code..."
if [[ -f "$CLAUDE_DIR/settings.json" ]]; then
    echo "   Backing up existing settings to settings.json.bak"
    cp "$CLAUDE_DIR/settings.json" "$CLAUDE_DIR/settings.json.bak"
    
    # Merge settings using Python
    python3 << 'PYTHON'
import json
import sys

existing_path = "$HOME/.claude/settings.json".replace("$HOME", "$HOME")
import os
existing_path = os.path.expandvars("$HOME/.claude/settings.json")
new_path = os.path.expandvars("$SCRIPT_DIR/settings.json")

try:
    with open(existing_path.replace("$SCRIPT_DIR", os.environ.get("SCRIPT_DIR", "."))) as f:
        existing = json.load(f)
except:
    existing = {}

# For now, just copy the new settings
# A more sophisticated merge could be done here
PYTHON
fi

cp "$SCRIPT_DIR/settings.json" "$CLAUDE_DIR/settings.json"

# 5. Install launchd plist
echo "🚀 Setting up daemon auto-start..."
LAUNCH_AGENTS="$HOME/Library/LaunchAgents"
mkdir -p "$LAUNCH_AGENTS"

if [[ -f "$SCRIPT_DIR/../integration/com.treerag.daemon.plist" ]]; then
    cp "$SCRIPT_DIR/../integration/com.treerag.daemon.plist" "$LAUNCH_AGENTS/"
    echo "   Installed launchd plist"
elif [[ -f "$SCRIPT_DIR/com.treerag.daemon.plist" ]]; then
    cp "$SCRIPT_DIR/com.treerag.daemon.plist" "$LAUNCH_AGENTS/"
    echo "   Installed launchd plist"
else
    echo "   ⚠️  No launchd plist found - manual daemon start required"
fi

echo ""
echo "═══════════════════════════════════════════"
echo "✓ Installation complete!"
echo ""
echo "Installed to: $INSTALL_DIR"
echo ""
echo "Next steps:"
echo "  1. Build the daemon:"
echo "     cd $(dirname "$SCRIPT_DIR") && cargo build --release"
echo ""
echo "  2. Start the daemon:"
echo "     cargo run --bin treerag-daemon"
echo ""
echo "  3. In Claude Code, use /init-project to index your project"
echo ""
echo "═══════════════════════════════════════════"
