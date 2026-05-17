# Posixfy Bridge

A low-level file service that performs filesystem operations with Unix UID/GID identity switching. The OS kernel enforces permission isolation — the service itself has no concept of users, only `setfsuid`/`setfsgid`.

> **Posixfy Bridge** 是一个底层文件服务，通过 Unix UID/GID 身份切换执行文件操作。操作系统内核强制隔离权限——服务本身没有用户概念，只有 `setfsuid`/`setfsgid`。

## Features

- `setfsuid`/`setfsgid` identity switching for all file operations
- Application-level file locking with TTL expiry
- Optimistic concurrency control (OCC) via mtime + size headers
- SSE-based real-time file change notifications (polling, zero overhead when idle)
- HTTP Range header support for partial downloads
- Paginated directory listing
- Path traversal defense (component check + canonicalize prefix check)
- Linux only — requires root for identity switching

## Quick Start

### Prerequisites

- Rust 1.75+
- Linux (requires `setfsuid`/`setfsgid` system calls)
- Root privileges (required for identity switching)

### Using Docker

```bash
docker pull ghcr.io/posixfy/posixfy-bridge:latest

docker run -d \
  --name posixfy-bridge \
  -p 3000:3000 \
  -e API_KEY=your-secret-key \
  -e MOUNT_POINTS=data:/data \
  -v /path/to/data:/data \
  ghcr.io/posixfy/posixfy-bridge:latest
```

### Manual Setup

```bash
cargo build --release

API_KEY=dev-key \
MOUNT_POINTS=data:/tmp/filebridge-data \
RUST_LOG=info \
./target/release/posixfy-bridge
```

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `API_KEY` | Yes | — | API key for authentication |
| `BRIDGE_LISTEN_ADDR` | No | `127.0.0.1:3000` | Bind address |
| `MOUNT_POINTS` | Yes | — | Named mount points: `name:/path,name2:/path2` |
| `RUST_LOG` | No | `info` | Log level |

## API

All endpoints require `X-API-Key: <API_KEY>` header. File operation endpoints additionally require identity headers.

### Health

```
GET /health
```

Returns `ok` if the service is running.

### Mount Points

```
GET /api/mounts
```

Returns the list of configured mount points.

### File Operations

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/fs/list?mount=&path=&page=&limit=` | List directory contents (paginated) |
| `GET` | `/api/fs/file?mount=&path=` | Download a file (supports `Range` header) |
| `POST` | `/api/fs/upload?mount=&path=` | Upload a file (multipart) |
| `DELETE` | `/api/fs/delete?mount=&path=` | Delete a file or directory |
| `POST` | `/api/fs/mkdir?mount=&path=` | Create a directory |
| `POST` | `/api/fs/rename` | Rename/move a file. Body: `{ "mount", "from", "to" }` |

### Headers for File Operations

| Header | Required | Description |
|--------|----------|-------------|
| `X-FS-UID` | Yes | Unix user ID (must be > 0) |
| `X-FS-GID` | Yes | Unix group ID |
| `X-FS-Groups` | No | Comma-separated supplementary groups |
| `X-Expected-MTime` | No | OCC: expected file modification time (nanoseconds) |
| `X-Expected-Size` | No | OCC: expected file size in bytes |

### File Change Notifications

```
GET /api/fs/watch?mount=&path=
```

Subscribe to SSE events for a directory. Events: `created`, `modified`, `deleted`.

### Error Format

All errors return `{ "error": "description" }` with appropriate HTTP status codes:

| Code | Meaning |
|------|---------|
| 400 | Bad request (missing params, invalid path) |
| 401 | Missing or invalid API key |
| 403 | Forbidden (uid=0 rejected) |
| 404 | File not found |
| 409 | Conflict (OCC check failed, file modified) |
| 416 | Range not satisfiable |
| 500 | Internal server error |

## Architecture

```
Client ──[HTTP + X-API-Key + X-FS-UID]──▶ Posixfy Bridge
                                           │
                                           ├── FsUidGuard (setfsuid/setfsgid)
                                           ├── LockManager (DashMap + TTL)
                                           ├── PollingWatcher (readdir + stat)
                                           └── std::fs (spawn_blocking)
```

- **No database** — stateless service, all state in memory
- **Identity switching** — each request runs as the specified Unix user
- **Blocking ops isolation** — all file operations run in `spawn_blocking`
- **Zero-overhead watching** — polling only when SSE subscribers exist

## Project Structure

```
posixfy-bridge/
├── src/
│   ├── main.rs           # Entry point, AppState, router
│   ├── config.rs         # Environment variable config
│   ├── error.rs          # AppError unified error
│   ├── api/              # HTTP endpoints
│   │   ├── fs.rs         # File operations
│   │   ├── mounts.rs     # Mount points listing
│   │   └── watch.rs      # SSE file change notifications
│   ├── auth/             # API key + identity extractors
│   │   └── middleware.rs
│   ├── fs/               # Filesystem internals
│   │   ├── guard.rs      # FsUidGuard RAII
│   │   ├── lock.rs       # LockManager
│   │   ├── operations.rs # Blocking file ops
│   │   ├── path.rs       # Path validation
│   │   └── watcher.rs    # Polling-based file watcher
│   └── models/
│       └── mount.rs      # MountPoint model
├── tests/
│   └── api_test.rs       # Integration tests
├── Cargo.toml
├── Dockerfile
└── Makefile
```

## Security

- Run behind a reverse proxy or internal network — do not expose directly to the internet
- Use a strong, randomly generated `API_KEY`
- The service must run as root (for `setfsuid`/`setfsgid`), but file operations execute as the specified user
- Path traversal is blocked via `..` component rejection and canonicalize prefix check
- File uploads have filename sanitization (path components stripped, 255-byte limit)

## Development

```bash
make dev          # Run with default dev config
make test         # Run all tests
make lint         # cargo fmt + clippy
cargo test        # Run tests with output
cargo test -- --nocapture  # Run tests with print output
```

## License

Licensed under the Apache License, Version 2.0.
