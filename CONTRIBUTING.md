# Contributing to Posixfy Bridge

Thank you for your interest in contributing to Posixfy Bridge!

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch
4. Make your changes
5. Run tests: `make test`
6. Run linting: `make lint`
7. Commit and push
8. Open a Pull Request

## Development Setup

### Prerequisites
- Rust 1.75+
- Linux (required for `setfsuid`/`setfsgid`)
- Root privileges for running

### Quick Start
```bash
cargo build --release
make dev       # Run with dev config
make test      # Run tests
make lint      # cargo fmt + clippy
```

## Code Style

- **Formatting**: `cargo fmt`
- **Linting**: `cargo clippy -- -D warnings`
- **Tests**: Write unit tests alongside code. Integration tests go in `tests/`.

## Pull Request Guidelines

- Focused PRs — one logical change per PR
- Include tests for new behavior
- Update documentation if behavior changes
- Ensure CI passes (fmt, clippy, tests)
- Clear commit messages

## Reporting Issues

Use GitHub Issues. Include reproduction steps and environment details. Report security vulnerabilities privately.

## License

Contributions are licensed under the Apache License, Version 2.0.
