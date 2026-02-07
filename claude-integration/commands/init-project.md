Initialize the current project for TreeRAG smart context.

This command will:
1. Scan the codebase structure (fast, ~30 seconds)
2. Parse code for symbols and dependencies
3. Start background AI enrichment (optional)

Usage: /init-project [--quick]

Options:
  --quick  Skip AI enrichment (faster, less detailed)
