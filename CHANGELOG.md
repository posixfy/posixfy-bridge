# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- File modification times (`modified`) are now emitted as epoch **milliseconds** instead of seconds. This fixes timestamps that appeared near 1970 in clients that pass the value to JavaScript's `Date`. Affects directory listings, file `stat`, and SSE change events.
- Renamed remaining legacy "FileBridge" references to "Posixfy" (LICENSE copyright holder, README example mount path, test helper doc comment). No functional change.

### Added
- Structured `tracing` logging for every filesystem operation (list, read, write, delete, mkdir, rename), including the target path and acting UID — mutating ops at `info`, reads at `debug`, failures at `error`. All error responses are now logged (4xx at `warn`, 5xx at `error`); previously only IO errors were logged.

### Fixed
- `delete` is now idempotent: removing an already-absent path (e.g. a `.DS_Store` recreated or removed by an external client) is treated as success instead of returning an error.

## [0.1.0] - 2026-05-17

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
