# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building and Testing
```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run specific test module
cargo test gcra_algorithm_tests

# Run tests with output (useful for debugging)
cargo test -- --nocapture

# Run single-threaded tests (for timing-sensitive tests)
cargo test -- --test-threads=1
```

### Code Quality
```bash
# Format code
cargo fmt

# Check formatting without changing files
cargo fmt --check

# Run clippy linter
cargo clippy

# Run clippy with all warnings as errors
cargo clippy -- -D warnings

# Generate and open documentation
cargo doc --open

# Test documentation examples
cargo test --doc
```

## Architecture Overview

Flux Limiter is a high-performance rate limiter implementing the Generic Cell Rate Algorithm (GCRA) with nanosecond precision.

### Core Components

- **FluxLimiter<T, C>**: Main rate limiter struct with generic client ID type `T` and clock `C`
- **FluxLimiterConfig**: Configuration management with builder pattern support
- **FluxLimiterDecision**: Rich metadata returned from rate limiting decisions
- **FluxLimiterError**: Comprehensive error handling for clock and configuration issues
- **Clock abstraction**: Pluggable time sources (SystemClock for production, TestClock for testing)

### Key Modules

- `src/lib.rs`: Main library exports and documentation
- `src/flux_limiter.rs`: Core GCRA algorithm implementation
- `src/config.rs`: Configuration types and validation
- `src/errors.rs`: Error types and handling
- `src/clock.rs`: Clock abstraction and implementations
- `tests/ratelimiter/`: Comprehensive integration test suite

### Design Principles

1. **Lock-free concurrency**: Uses DashMap for thread-safe, lock-free client state storage
2. **Nanosecond precision**: All timing calculations use u64 nanoseconds to avoid floating-point drift
3. **Generic client IDs**: Supports String, IpAddr, u64, or any Hash + Eq + Clone type
4. **Rich metadata**: Returns detailed decision information for HTTP headers and monitoring
5. **Graceful error handling**: Clock failures are handled gracefully with Result types

## Test Organization

Tests are organized in `tests/ratelimiter/` with these modules:
- `gcra_algorithm_tests.rs`: Core algorithm correctness
- `config_tests.rs`: Configuration validation
- `error_tests.rs`: Error handling and recovery
- `cleanup_tests.rs`: Memory management
- `performance_tests.rs`: Performance characteristics
- `decision_metadata_tests.rs`: Decision metadata validation
- `fixtures/test_clock.rs`: TestClock for deterministic testing

### Test Utilities

Use `TestClock` for deterministic testing:
```rust
use crate::fixtures::test_clock::TestClock;

let clock = TestClock::new(0.0);
clock.advance(1.0); // Advance by 1 second
clock.fail_next_call(); // Simulate clock failure
```

## Common Development Patterns

### Creating Rate Limiters
```rust
// With builder pattern
let config = FluxLimiterConfig::new(0.0, 0.0)
    .rate(100.0)        // 100 requests per second
    .burst(50.0);       // Allow bursts of up to 50 requests

let limiter = FluxLimiter::with_config(config, SystemClock)?;
```

### Error Handling
Always handle clock errors gracefully:
```rust
match limiter.check_request(client_id) {
    Ok(decision) => {
        if decision.allowed {
            // Process request
        } else {
            // Rate limited - use decision.retry_after_seconds
        }
    }
    Err(FluxLimiterError::ClockError(_)) => {
        // Implement your policy: fail-open, fail-closed, or fallback
    }
    Err(e) => {
        // Configuration errors shouldn't happen at runtime
    }
}
```

### Memory Management
Periodically clean up stale clients:
```rust
let one_hour_nanos = 60 * 60 * 1_000_000_000u64;
let _ = limiter.cleanup_stale_clients(one_hour_nanos);
```

## Performance Considerations

- `check_request()` is O(1) with nanosecond precision
- Memory usage is O(number of active clients)
- Lock-free concurrent access via DashMap
- Hot path avoids allocations
- Use cleanup_stale_clients() periodically to prevent memory growth

## Code Style Guidelines

- Follow standard Rust conventions (snake_case, PascalCase, etc.)
- Use `cargo fmt` for consistent formatting
- Fix all `cargo clippy` warnings
- Document all public APIs with rustdoc comments
- Include examples in documentation
- Use Result types for error handling (never panic in library code)
- Prefer safe code over unsafe
- Write comprehensive tests for all public APIs

## Testing Requirements

- All public APIs must have tests
- Error conditions must be tested
- Use TestClock for deterministic timing tests
- Test edge cases and boundary conditions
- Validate performance characteristics
- Test with different client ID types (String, IpAddr, u64)