# Engram CLI Commands

## Daemon Control

### `engram start`
Start the Engram daemon in the background.

```bash
engram start
```

### `engram stop`
Stop the running daemon.

```bash
engram stop
```

### `engram status`
Show daemon status and metrics.

```bash
engram status
```

Output:
```
Engram Daemon Status
  Version: 0.1.0
  Uptime: 3600s
  Projects loaded: 2
  Memory: 45MB
  Requests: 1234
  Cache hit rate: 92%
```

### `engram ping`
Check daemon responsiveness.

```bash
engram ping
```

Output:
```
Pong! (2ms)
```

## Project Management

### `engram init <path>`
Initialize a project for indexing.

```bash
engram init .
engram init /path/to/project
```

Options:
- `--quick`: Skip AI enrichment (faster)

### `engram project <path>`
Show project information.

```bash
engram project .
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
| `ENGRAM_SOCKET` | `/tmp/engram.sock` | Unix socket path |
| `ENGRAM_DATA_DIR` | `~/.engram` | Data directory |
| `ENGRAM_LOG_LEVEL` | `info` | Log level (trace/debug/info/warn/error) |
