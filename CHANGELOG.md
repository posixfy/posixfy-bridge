# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Initial release of Posixfy Bridge
- File operations: list, download, upload, delete, mkdir, rename
- `setfsuid`/`setfsgid` identity switching via FsUidGuard
- Application-level file locking with TTL expiry
- Optimistic concurrency control (OCC)
- SSE-based real-time file change notifications
- HTTP Range header support
- Paginated directory listing
- Path traversal defense
- Filename sanitization on upload
- Integration test suite
