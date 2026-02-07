# TreeRAG CLI Commands

## Daemon Control

### `treerag start`
Start the TreeRAG daemon in the background.

```bash
treerag start
```

### `treerag stop`
Stop the running daemon.

```bash
treerag stop
```

### `treerag status`
Show daemon status and metrics.

```bash
treerag status
```

Output:
```
TreeRAG Daemon Status
  Version: 0.1.0
  Uptime: 3600s
  Projects loaded: 2
  Memory: 45MB
  Requests: 1234
  Cache hit rate: 92%
```

### `treerag ping`
Check daemon responsiveness.

```bash
treerag ping
```

Output:
```
Pong! (2ms)
```

## Project Management

### `treerag init <path>`
Initialize a project for indexing.

```bash
treerag init .
treerag init /path/to/project
```

Options:
- `--quick`: Skip AI enrichment (faster)

### `treerag project <path>`
Show project information.

```bash
treerag project .
```

Output:
```
Project: my-project
  Path: /path/to/project
  Files: 1234
  Symbols: 5678
  Last indexed: 2024-01-15 10:30:00
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TREERAG_SOCKET` | `/tmp/treerag.sock` | Unix socket path |
| `TREERAG_DATA_DIR` | `~/.treerag` | Data directory |
| `TREERAG_LOG_LEVEL` | `info` | Log level (trace/debug/info/warn/error) |
