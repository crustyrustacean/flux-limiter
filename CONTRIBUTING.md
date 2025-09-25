# Contributing to Flux Limiter

Thank you for your interest in contributing to Flux Limiter! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Code Standards](#code-standards)
- [Testing Guidelines](#testing-guidelines)
- [Documentation](#documentation)
- [Performance Considerations](#performance-considerations)
- [Submitting Changes](#submitting-changes)
- [Release Process](#release-process)

## Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors. Please be respectful and constructive in all interactions.

## Getting Started

### Prerequisites

- **Rust**: Latest stable version (check with `rustc --version`)
- **Cargo**: Comes with Rust
- **Git**: For version control

### Setting Up Your Development Environment

1. **Fork and clone the repository**:
   ```bash
   git clone https://github.com/YOUR_USERNAME/flux-limiter.git
   cd flux-limiter
   ```

2. **Build the project**:
   ```bash
   cargo build
   ```

3. **Run the test suite**:
   ```bash
   cargo test
   ```

4. **Check code formatting**:
   ```bash
   cargo fmt --check
   ```

5. **Run clippy for linting**:
   ```bash
   cargo clippy -- -D warnings
   ```

### Project Structure

```
flux-limiter/
├── src/
│   ├── lib.rs              # Main library exports
│   ├── flux_limiter.rs     # Core rate limiter implementation
│   ├── config.rs           # Configuration types
│   ├── errors.rs           # Error handling
│   └── clock.rs            # Clock abstraction
├── tests/
│   └── ratelimiter/        # Integration tests
│       ├── fixtures/       # Test utilities
│       └── *.rs           # Test modules
├── Cargo.toml
├── README.md
└── CONTRIBUTING.md
```

## Development Workflow

### Branch Naming

Use descriptive branch names with prefixes:
- `feature/add-new-algorithm` - New features
- `fix/clock-error-handling` - Bug fixes  
- `docs/update-readme` - Documentation updates
- `refactor/simplify-config` - Code refactoring
- `perf/optimize-cleanup` - Performance improvements

### Commit Messages

Follow conventional commit format:
```
type(scope): brief description

Longer explanation if needed

Fixes #123
```

Examples:
- `feat(core): add support for custom client ID types`
- `fix(clock): handle system time going backwards`
- `docs(readme): add error handling examples`
- `test(cleanup): add comprehensive cleanup tests`
- `perf(gcra): optimize nanosecond calculations`

## Code Standards

### Rust Style Guidelines

1. **Formatting**: Use `cargo fmt` for consistent formatting
2. **Linting**: Fix all `cargo clippy` warnings
3. **Naming**: Follow Rust naming conventions
   - `snake_case` for functions, variables, modules
   - `PascalCase` for types, traits, enums
   - `SCREAMING_SNAKE_CASE` for constants

### Code Quality Standards

1. **Error Handling**:
   - Use `Result<T, E>` for fallible operations
   - Provide meaningful error messages
   - Document error conditions in function docs

2. **Documentation**:
   - All public items must have doc comments
   - Include examples for public APIs
   - Document panics, errors, and safety requirements

3. **Safety**:
   - Minimize use of `unsafe` code
   - Document any `unsafe` blocks with safety invariants
   - Prefer safe alternatives when possible

4. **Performance**:
   - Avoid allocations in hot paths
   - Use appropriate data structures
   - Consider concurrent access patterns

### Example Code Style

```rust
/// Checks if a request should be allowed for the given client.
///
/// This method implements the GCRA algorithm to determine if the request
/// conforms to the configured rate and burst limits.
///
/// # Arguments
///
/// * `client_id` - Unique identifier for the client making the request
///
/// # Returns
///
/// * `Ok(FluxLimiterDecision)` - Rate limiting decision with metadata
/// * `Err(FluxLimiterError)` - Clock error or configuration issue
///
/// # Examples
///
/// ```rust
/// use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};
///
/// let config = FluxLimiterConfig::new(10.0, 5.0);
/// let limiter = FluxLimiter::with_config(config, SystemClock)?;
///
/// match limiter.check_request("user_123") {
///     Ok(decision) if decision.allowed => {
///         // Process request
///     }
///     Ok(_) => {
///         // Rate limited
///     }
///     Err(e) => {
///         // Handle error
///     }
/// }
/// ```
pub fn check_request(&self, client_id: T) -> Result<FluxLimiterDecision, FluxLimiterError> {
    let current_time_nanos = self.clock.now().map_err(FluxLimiterError::ClockError)?;
    
    // Implementation...
}
```

## Testing Guidelines

### Test Organization

Tests are organized in the `tests/ratelimiter/` directory:
- **Unit tests**: Test individual functions/methods
- **Integration tests**: Test complete workflows
- **Error tests**: Test error conditions and recovery
- **Performance tests**: Test performance characteristics

### Writing Tests

1. **Test Structure**:
   ```rust
   #[test]
   fn descriptive_test_name() {
       // Arrange
       let clock = TestClock::new(0.0);
       let config = FluxLimiterConfig::new(10.0, 5.0);
       let limiter = FluxLimiter::with_config(config, clock).unwrap();
       
       // Act
       let result = limiter.check_request("client1");
       
       // Assert
       assert!(result.is_ok());
       assert!(result.unwrap().allowed);
   }
   ```

2. **Test Coverage Requirements**:
   - All public APIs must have tests
   - Error conditions must be tested
   - Edge cases should be covered
   - Performance characteristics should be validated

3. **Test Data**:
   - Use meaningful test data
   - Test boundary conditions
   - Test with realistic client ID types

### Using Test Fixtures

Use the provided `TestClock` for deterministic testing:

```rust
use crate::fixtures::test_clock::TestClock;

#[test]
fn test_time_progression() {
    let clock = TestClock::new(0.0);
    
    // First request
    let result1 = limiter.check_request("client1").unwrap();
    assert!(result1.allowed);
    
    // Advance time and test again
    clock.advance(1.0);
    let result2 = limiter.check_request("client1").unwrap();
    assert!(result2.allowed);
}
```

### Error Testing

Test error conditions using `TestClock` failure simulation:

```rust
#[test]
fn test_clock_error_handling() {
    let clock = TestClock::new(0.0);
    let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
    
    clock.fail_next_call();
    let result = limiter.check_request("client1");
    assert!(result.is_err());
}
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test gcra_algorithm_tests

# Run with output
cargo test -- --nocapture

# Run performance tests
cargo test performance_tests

# Test with different client ID types
cargo test -- --test-threads=1  # For timing-sensitive tests
```

## Documentation

### Documentation Requirements

1. **API Documentation**:
   - All public items must have rustdoc comments
   - Include usage examples
   - Document error conditions
   - Explain performance characteristics

2. **README Updates**:
   - Update examples for new features
   - Add new configuration options
   - Update performance claims with benchmarks

3. **CHANGELOG**:
   - Follow [Keep a Changelog](https://keepachangelog.com/) format
   - Document breaking changes
   - Include migration guidance

### Documentation Commands

```bash
# Generate and open documentation
cargo doc --open

# Check for missing documentation
cargo doc --document-private-items

# Test documentation examples
cargo test --doc
```

## Performance Considerations

### Performance Requirements

- `check_request()` should complete in O(1) time
- Memory usage should be O(number of active clients)
- Lock-free operations preferred for hot paths
- Nanosecond precision must be maintained

### Benchmarking

When making performance-related changes:

1. **Before/After Benchmarks**:
   ```bash
   # Create simple benchmark
   cargo test performance_tests --release -- --nocapture
   ```

2. **Memory Usage**:
   - Monitor memory growth in long-running tests
   - Test cleanup effectiveness

3. **Concurrency**:
   - Test with multiple threads
   - Verify lock-free behavior

### Performance Guidelines

- Avoid allocations in `check_request()`
- Use efficient data structures (DashMap vs HashMap)
- Minimize atomic operations
- Consider cache locality for hot data

## Submitting Changes

### Pull Request Process

1. **Before Creating a PR**:
   - Ensure all tests pass: `cargo test`
   - Run formatting: `cargo fmt`
   - Fix clippy warnings: `cargo clippy -- -D warnings`
   - Update documentation if needed
   - Add tests for new functionality

2. **Creating the PR**:
   - Use descriptive title and description
   - Reference related issues
   - Include test results
   - Explain design decisions for complex changes

3. **PR Description Template**:
   ```markdown
   ## Summary
   Brief description of changes
   
   ## Changes Made
   - List of specific changes
   - New features added
   - Bugs fixed
   
   ## Testing
   - Tests added/modified
   - Manual testing performed
   
   ## Breaking Changes
   - List any breaking changes
   - Migration guidance
   
   ## Related Issues
   Fixes #123
   ```

### Review Process

1. **Automated Checks**: CI will run tests, formatting, and linting
2. **Code Review**: Maintainers will review code quality and design
3. **Testing**: Verify tests are comprehensive and passing
4. **Documentation**: Check that documentation is updated appropriately

## Release Process

### Versioning

We follow [Semantic Versioning](https://semver.org/):
- **MAJOR**: Incompatible API changes
- **MINOR**: New functionality (backward compatible)  
- **PATCH**: Bug fixes (backward compatible)

### Release Checklist

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Ensure all tests pass
4. Update documentation
5. Create release tag
6. Publish to crates.io (maintainers only)

## Getting Help

- **Questions**: Open a discussion or issue on GitHub
- **Bugs**: Create an issue with reproduction steps
- **Feature Requests**: Open an issue with detailed description
- **Security Issues**: Contact maintainers privately

## Areas for Contribution

We welcome contributions in these areas:

### High Priority
- **Algorithm Optimizations**: Improve performance while maintaining correctness
- **Error Handling**: Enhance robustness and error recovery
- **Documentation**: Improve examples and guides
- **Testing**: Add more comprehensive test coverage

### Medium Priority
- **Platform Support**: Testing on different platforms
- **Integration Examples**: More framework integrations
- **Monitoring**: Better observability features
- **Configuration**: Additional configuration options

### Future Enhancements
- **Distributed Rate Limiting**: Multi-node coordination
- **Persistence**: Optional state persistence
- **Alternative Algorithms**: Support for other rate limiting algorithms
- **Async Support**: Native async/await integration

Thank you for contributing to Flux Limiter! Your contributions help make rate limiting more reliable and accessible for the Rust community.