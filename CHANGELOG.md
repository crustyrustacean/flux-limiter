# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0] - 2026-04-27

### Added
- `Clone` implementation for `FluxLimiter<T, C>` — clones share the same `DashMap` client state via `Arc`, providing lightweight sharing without needing `Arc<FluxLimiter<..>>`
- `client_count()` and `contains_client()` public accessor methods on `FluxLimiter`
- `Display` and `Error` trait implementations for `ClockError`
- `From<ClockError>` conversion for `FluxLimiterError`, enabling ergonomic `?` operator
- `source()` implementation on `FluxLimiterError::Error` for error chain inspection

### Changed
- `client_state` field visibility changed from `pub` to `pub(crate)` — use `client_count()` and `contains_client()` instead
- `FluxLimiterError::ClockError` display now chains the inner `ClockError` message
- Simplified clock error handling in `check_request()` and `cleanup_stale_clients()` to use `?` via `From` conversion
- Fixed stale file path comment in `src/flux_limiter.rs`
- Fixed documentation discrepancies in `docs/src/architecture/components.md` (struct derives, return types, Display strings, phantom `From` impl)
- Updated `docs/src/architecture/concurrency.md` and `docs/src/guide/basic-usage.md` to document `Clone` as an alternative to `Arc`

### Removed
- Dead code: `increment_nanos()`, `tolerance_nanos()`, `increment()`, and `tolerance()` internal methods

## [0.7.2] - 2025-12-20

### Changed
- Updated documentation dependency examples from 0.4.0 to 0.7.2 in README.md and installation guide
- Fixed GitHub repository URLs in mdbook configuration to point to correct `crustyrustacean` organization

## [0.7.1] - 2025-11-16

### Added
- Comprehensive mdbook documentation with separate chapters for architecture, guides, and API reference
- CONTRIBUTING.md with detailed contributor guidelines
- CHANGELOG.md to track version history
- Homepage URL in package metadata pointing to GitHub Pages documentation
- Documentation URL in package metadata pointing to docs.rs

### Changed
- Condensed README.md from 356 lines to 70 lines for better readability
- README.md now focuses on quick start and links to comprehensive documentation
- Updated CONTRIBUTING.md to reference new mdbook documentation structure

### Removed
- ARCHITECTURE.md (content migrated to mdbook documentation)

## [0.6.3] - 2025-11-16

### Changed
- Removed AI guardrails documentation
- Fixed rogue "testing" feature mention in `Cargo.toml`

## [0.6.2] - 2025-11-16

### Changed
- Prepared package for publishing to crates.io
- Updated dependencies to latest versions

## [0.6.1] - 2025-11-15

### Added
- Comprehensive error handling for clock operations
- Integration tests for error handling and recovery
- Rich metadata in `FluxLimiterDecision` for HTTP headers and monitoring
- `retry_after_seconds` field for rate-limited responses
- `remaining_capacity` field for burst capacity tracking
- `reset_time_nanos` field for rate limit window reset time

### Changed
- Refactored test suite into integration tests pattern
- Organized tests into separate modules (GCRA algorithm, config, errors, cleanup, performance)
- Moved test fixtures into dedicated `fixtures/` directory
- Improved documentation and examples

## [0.6.0] - 2025-09-21

### Added
- Configuration system with `FluxLimiterConfig` and builder pattern
- `cleanup_stale_clients()` method for memory management
- Enhanced security with proper configuration validation
- Comprehensive rustdoc documentation

### Changed
- Improved configuration ergonomics with builder pattern
- Enhanced documentation with detailed examples

## [0.5.0] - 2025-09-21

### Added
- Initial implementation of GCRA-based rate limiter
- Lock-free concurrent access using DashMap
- Generic client ID support (String, IpAddr, u64, custom types)
- Nanosecond precision timing
- Clock abstraction for testability
- `SystemClock` for production use
- Thread-safe operation across multiple threads
- Zero-allocation hot path for performance

### Features
- Mathematically precise GCRA algorithm implementation
- O(1) time complexity for rate limit checks
- O(number of active clients) memory usage
- Support for configurable rate and burst capacity

[Unreleased]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/crustyrustacean/flux-limiter/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/crustyrustacean/flux-limiter/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/crustyrustacean/flux-limiter/compare/v0.6.3...v0.7.1
[0.6.3]: https://github.com/crustyrustacean/flux-limiter/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/crustyrustacean/flux-limiter/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/crustyrustacean/flux-limiter/releases/tag/v0.6.1
[0.6.0]: https://github.com/crustyrustacean/flux-limiter/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/crustyrustacean/flux-limiter/releases/tag/v0.5.0
