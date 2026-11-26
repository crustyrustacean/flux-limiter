# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.9.0] - 2026-11-25

### Fixed
- A backwards step of the operating-system clock (an NTP correction, a manual change, a VM resuming with a stale clock) no longer denies every request until the clock catches back up. The limiter's default clock measures elapsed time instead of reading the wall clock, so decisions depend on how much time really passed and capacity refills correctly while the OS clock is wrong (#5)
- The documented workaround for that problem, a clock that clamps its readings so they never go backwards, is removed from the guide. It did not fix the problem: the clamp froze the limiter's time at the instant of the step, so nothing refilled until the OS clock climbed back past it. Measured, a client that waited 9.9 real seconds after a 10-second step got none of the requests it had earned

### Changed
- **The default clock is now `MonotonicClock`** instead of `SystemClock`. `FluxLimiter<K>` means `FluxLimiter<K, MonotonicClock>`; nothing else changes for callers that do not name a clock. `SystemClock` is unchanged and remains available for callers who need readings that match the OS clock exactly; pass it explicitly to keep wall-clock behaviour and accept its limitation
- `MonotonicClock::new()` is required to write the new clock by name, as in `FluxLimiter::with_config(config, MonotonicClock::new())`. `SystemClock` can still be written bare. This is the one place where the new clock is not a drop-in replacement
- Each `check_request()` costs about 3 ns more with the new default: measured at 49.8 ns with `SystemClock` and 53.0 ns with `MonotonicClock`, same build, only the clock differing. Reading a monotonic clock costs slightly more than reading the wall clock on this platform, and the anchor adds one atomic load. Decisions are identical for both clocks
- `FluxLimiter`'s clock field is 40 bytes instead of zero, so the struct is no longer 24 bytes. Pass `SystemClock` if that matters to you and you accept its limitation
- Every code example in the documentation uses `MonotonicClock`, so no copied example is left on the clock that has the backwards-step problem. `SystemClock` appears only where it is itself the subject
- The documentation is checked against the code as of this release, and no longer overstates what the crate does. Memory and latency figures are replaced with measured values (about 31 bytes per client with a `u64` key, growing in steps rather than linearly), the rate interval is described as truncated to a whole nanosecond rather than exact, cleanup is described as something you call rather than something that runs on its own, and the one case where an update does wait on a lock (after 64 lost compare-and-swap races) is stated

### Added
- `MonotonicClock`, the new default clock. It reads the OS clock once, on first use, to record an anchor, then reports `anchor + elapsed`. Readings are Unix-epoch nanoseconds that never go backwards; they track the OS clock to within microseconds for as long as that clock is not stepped, so `reset_time_nanos` still works in headers such as `X-RateLimit-Reset` without conversion. It never returns an error
- Tests for the clock's monotonicity under contention, anchor sharing between clones, invariance to a backwards step, refill by real elapsed time, and epoch accuracy

## [0.8.6] - 2026-11-25

### Changed
- `check_request()` no longer takes a lock when updating a client it has already seen. Each client's TAT is stored as an `AtomicU64` and published with a compare-and-swap, so requests for the same client no longer queue behind the map shard's write lock, and requests for different clients in the same shard no longer contend for it. A client is still inserted under the shard lock the first time it is seen. The public API is unchanged, as is the set of decisions returned for any sequence of requests
- Same-client contention is 1.6 to 2.4 times faster: with 4 threads hitting one client, 159 ns per request down to 67–96 ns across repeated runs on one machine (that figure varies by about 40% between runs, so the range is the result). Requests for distinct clients are unaffected
- A request that repeatedly loses the compare-and-swap race retries a bounded number of times and then settles under the write lock, so a hot client cannot make a request spin indefinitely

### Added
- `contention_hot_key_u64` and `contention_spread_keys_u64` benchmarks, measured per request with worker threads started outside the timed region
- Multi-threaded tests for same-client contention, including one that verifies every concurrent write lands in the TAT. The suite previously had no concurrent test

## [0.8.5] - 2026-11-25

### Fixed
- `FluxLimiterConfig::validate()` now rejects non-finite values. Previously `NaN` passed every check and `INFINITY` was accepted as a rate, producing a limiter with a zero rate interval that never limited anything (#4)
- Rates above 1,000,000,000 per second are rejected with `InvalidRate`. The rate interval is stored as whole nanoseconds, so any higher rate truncated to a zero interval and imposed no limit (#4)
- A finite burst whose tolerance (`burst × interval`) would not fit in 64 bits is rejected at construction with `InvalidBurst`. Previously the value saturated to `u64::MAX`, giving an effectively unlimited burst and an overflow in `cleanup_stale_clients()` (#4)
- A tiny rate (e.g. `1e-12`) is rejected. Previously its interval saturated to `u64::MAX` and the first request from any client overflowed — a panic in debug builds, and a timestamp in the past in release builds (#4)
- All three TAT computations now use saturating addition instead of unchecked `+`. Previously an overflow panicked in debug builds and wrapped silently in release builds, where a wrapped value in cleanup could evict a live client. The documentation already described saturating arithmetic; the code now matches it (#4)

### Changed
- Out-of-range values reuse the existing `InvalidRate` / `InvalidBurst` variants; no new error variants were added, so existing `match` arms are unaffected
- `TestClock::set_nanos(u64)` added to the test fixtures, for setting an exact nanosecond value that `set_time(f64)` cannot represent
- Documented that the rate interval and burst tolerance are truncated to whole nanoseconds, with a table of requested vs. enforced rate (a rate of 6e8 is enforced at 1e9, +67%). Rates that divide 1e9 evenly are enforced exactly. The arithmetic is unchanged

## [0.8.4] - 2026-11-25

### Added
- Criterion benchmark suite (`benches/flux_limiter_bench.rs`) — measures the `check_request()` hot path in three scenarios: single-thread `u64` key, single-thread `String` key (clone), and 4 threads contending on one client. Establishes a measured baseline for upcoming performance work
- `.gitattributes` with `* text=auto eol=lf` — line endings are now a property of the repository rather than of each machine's `core.autocrlf`, so the working tree is identical on Windows and Linux

### Changed
- Added `criterion = "0.8"` as a dev-dependency (dev-only; does not affect the published library or its dependency tree)

### Fixed
- `.gitignore` was missing a trailing newline

## [0.8.3] - 2026-07-11

### Fixed
- Fixed broken documentation link in README.md — `[full documentation](./docs/src/SUMMARY.md)` was a relative repo path that 404s on crates.io; now points to `https://docs.rs/flux-limiter` (#4)

### Changed
- Added `vendor/` to `.gitignore` — vendored dependencies were local-only (never tracked in git, never published) but not ignored

## [0.8.2] - 2026-07-11

### Added
- GitHub Actions CI workflow (`.github/workflows/ci.yaml`) — runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --all-targets`, and `cargo audit` on push and pull request to main (#9)
- Trust badges in README.md — crates.io version, docs.rs, MIT license, Rust edition 2024, and CI status (#5)

### Fixed
- Corrected `cleanup_stale_clients()` return type in documentation — was shown as `Ok(count)` / `Ok(removed_count)` returning `usize`, but the method returns `Result<(), FluxLimiterError>`. Fixed across error-handling guide, advanced-usage guide, web-integration guide, production guide, error-handling architecture, testing architecture, and future-extensibility pages
- Fixed `FluxLimiterError::ClockError` Display in `components.md` to chain the inner error message (`"Clock error occurred: {}"`), matching the actual implementation
- Fixed GCRA algorithm pseudocode in `gcra-algorithm.md` — `calculate_remaining_capacity()` now matches the actual implementation; `reset_time_nanos` correctly shown as `new_tat_nanos` (not `new_tat_nanos + tolerance_nanos`)
- Fixed `README.md` field name: `remaining_after_capacity` → `remaining_capacity` (matching the actual `FluxLimiterDecision` field)
- Fixed version number in installation guide: `0.8.0` → `0.8.1` (now `0.8.2`)

### Changed
- Removed unsupported "high-performance" and "zero allocations" claims from README.md, `docs/book.toml`, and all mdbook documentation pages — no in-repo benchmarks exist to substantiate these claims
- Removed unverified benchmark numbers (specific ns/iter, throughput figures, scalability tables) from `performance.md`, `concurrency.md`, `overview.md`, and `design-decisions.md`
- Removed unsubstantiated ">95% code coverage" claim from `overview.md`
- Removed "Zero-Allocation Hot Path" design principle from `overview.md`
- Softened "High-performance numeric IDs" comment in `basic-usage.md` to "Numeric IDs avoid string allocation overhead"
- Updated `book.toml` description from "A high-performance rate limiter..." to "A rate limiter based on the Generic Cell Rate Algorithm (GCRA)"

## [0.8.1] - 2026-04-28

### Fixed
- Fixed TOCTOU race condition in `check_request()` — concurrent requests for the same client could both read the same TAT and be over-allowed. Now uses DashMap's `entry()` API for atomic read-modify-write (`Entry::Occupied`/`Entry::Vacant`) ensuring the conformance check and TAT update happen within a single shard lock (#1)
- Added `#[non_exhaustive]` to `FluxLimiterDecision` to prevent breaking changes when fields are added in future versions, consistent with `FluxLimiterError` (#2)

### Changed
- Updated documentation across concurrency, GCRA algorithm, performance, components, and quick-start pages to reflect the new `entry()` API pattern and `#[non_exhaustive]` attribute

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

[Unreleased]: https://github.com/crustyrustacean/flux-limiter/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.6...v0.9.0
[0.8.6]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.5...v0.8.6
[0.8.5]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.4...v0.8.5
[0.8.4]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.3...v0.8.4
[0.8.3]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.2...v0.8.3
[0.8.2]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/crustyrustacean/flux-limiter/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/crustyrustacean/flux-limiter/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/crustyrustacean/flux-limiter/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/crustyrustacean/flux-limiter/compare/v0.6.3...v0.7.1
[0.6.3]: https://github.com/crustyrustacean/flux-limiter/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/crustyrustacean/flux-limiter/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/crustyrustacean/flux-limiter/releases/tag/v0.6.1
[0.6.0]: https://github.com/crustyrustacean/flux-limiter/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/crustyrustacean/flux-limiter/releases/tag/v0.5.0