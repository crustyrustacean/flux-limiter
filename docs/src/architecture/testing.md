# Testing Architecture

Comprehensive testing strategy for deterministic and reliable tests.

## Test Organization

Tests are organized in `tests/ratelimiter/` with clear separation of concerns:

```
tests/ratelimiter/
├── fixtures/
│   ├── test_clock.rs      # TestClock implementation
│   └── mod.rs
├── gcra_algorithm_tests.rs # Core algorithm correctness
├── config_tests.rs        # Configuration validation
├── error_tests.rs         # Error handling and recovery
├── cleanup_tests.rs       # Memory management
├── performance_tests.rs   # Performance characteristics
├── decision_metadata_tests.rs # Decision metadata validation
└── main.rs               # Test module organization
```

## TestClock Design

The TestClock is the foundation for deterministic testing.

### Implementation

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

pub struct TestClock {
    time: Arc<AtomicU64>,        // Current time in nanoseconds
    should_fail: Arc<AtomicBool>, // Failure simulation flag
}

impl TestClock {
    pub fn new(initial_time_secs: f64) -> Self {
        Self {
            time: Arc::new(AtomicU64::new((initial_time_secs * 1e9) as u64)),
            should_fail: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn advance(&self, duration_secs: f64) {
        let duration_nanos = (duration_secs * 1e9) as u64;
        self.time.fetch_add(duration_nanos, Ordering::SeqCst);
    }

    pub fn set_time(&self, time_secs: f64) {
        self.time.store((time_secs * 1e9) as u64, Ordering::SeqCst);
    }

    pub fn fail_next_call(&self) {
        self.should_fail.store(true, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> Result<u64, ClockError> {
        if self.should_fail.swap(false, Ordering::SeqCst) {
            return Err(ClockError::SystemTimeError);
        }
        Ok(self.time.load(Ordering::SeqCst))
    }
}
```

### Key Features

1. **Deterministic Time**: Controlled time progression
2. **Thread-Safe**: Can be shared across test threads
3. **Failure Simulation**: Can simulate clock errors
4. **Precise Control**: Nanosecond-level manipulation

### Usage Example

```rust
#[test]
fn test_rate_limiting() {
    let clock = TestClock::new(0.0);
    let limiter = FluxLimiter::with_config(
        FluxLimiterConfig::new(10.0, 5.0),
        clock.clone(),
    ).unwrap();

    // First request at t=0
    assert!(limiter.check_request("client1").unwrap().allowed);

    // Advance time by 0.1 seconds
    clock.advance(0.1);

    // Second request should be allowed
    assert!(limiter.check_request("client1").unwrap().allowed);
}
```

## Test Categories

### 1. GCRA Algorithm Tests

Test core algorithm correctness:

```rust
#[test]
fn test_sustained_rate() {
    let clock = TestClock::new(0.0);
    let config = FluxLimiterConfig::new(10.0, 0.0); // 10 req/s, no burst
    let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

    // First request allowed
    assert!(limiter.check_request("client1").unwrap().allowed);

    // Request 0.05s later (too early)
    clock.advance(0.05);
    assert!(!limiter.check_request("client1").unwrap().allowed);

    // Request 0.1s after first (exactly on time)
    clock.advance(0.05);
    assert!(limiter.check_request("client1").unwrap().allowed);
}
```

### 2. Burst Capacity Tests

Verify burst handling:

```rust
#[test]
fn test_burst_capacity() {
    let clock = TestClock::new(0.0);
    let config = FluxLimiterConfig::new(10.0, 5.0); // 5 request burst
    let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

    // Should allow ~6 requests immediately (1 + burst)
    for _ in 0..6 {
        assert!(limiter.check_request("client1").unwrap().allowed);
    }

    // 7th request should be denied
    assert!(!limiter.check_request("client1").unwrap().allowed);

    // After rate interval, allow one more
    clock.advance(0.1);
    assert!(limiter.check_request("client1").unwrap().allowed);
}
```

### 3. Configuration Tests

Validate configuration handling:

```rust
#[test]
fn test_invalid_rate() {
    let config = FluxLimiterConfig::new(-10.0, 5.0);
    let result = FluxLimiter::with_config(config, MonotonicClock::new());

    assert!(matches!(result, Err(FluxLimiterError::InvalidRate)));
}

#[test]
fn test_invalid_burst() {
    let config = FluxLimiterConfig::new(10.0, -5.0);
    let result = FluxLimiter::with_config(config, MonotonicClock::new());

    assert!(matches!(result, Err(FluxLimiterError::InvalidBurst)));
}
```

### 4. Error Handling Tests

Test error scenarios and recovery:

```rust
#[test]
fn test_clock_error_handling() {
    let clock = TestClock::new(0.0);
    let limiter = FluxLimiter::with_config(
        FluxLimiterConfig::new(10.0, 5.0),
        clock.clone(),
    ).unwrap();

    // Normal operation
    assert!(limiter.check_request("client1").unwrap().allowed);

    // Simulate clock failure
    clock.fail_next_call();
    let result = limiter.check_request("client1");
    assert!(matches!(result, Err(FluxLimiterError::ClockError(_))));

    // Verify recovery
    assert!(limiter.check_request("client1").unwrap().allowed);
}

#[test]
fn test_multiple_clock_failures() {
    let clock = TestClock::new(0.0);
    let limiter = FluxLimiter::with_config(
        FluxLimiterConfig::new(10.0, 5.0),
        clock.clone(),
    ).unwrap();

    // Multiple consecutive failures
    for _ in 0..5 {
        clock.fail_next_call();
        assert!(limiter.check_request("client1").is_err());
    }

    // Recovery
    assert!(limiter.check_request("client1").unwrap().allowed);
}
```

### 5. Cleanup Tests

Test memory management:

```rust
#[test]
fn test_cleanup_stale_clients() {
    let clock = TestClock::new(0.0);
    let limiter = FluxLimiter::with_config(
        FluxLimiterConfig::new(10.0, 5.0),
        clock.clone(),
    ).unwrap();

    // Create some client state
    limiter.check_request("client1").unwrap();
    limiter.check_request("client2").unwrap();
    limiter.check_request("client3").unwrap();

    // Advance time by 1 hour
    clock.advance(3600.0);

    // Cleanup clients older than 30 minutes
    let threshold = 30 * 60 * 1_000_000_000u64;
    limiter.cleanup_stale_clients(threshold).unwrap();

    // All clients should be removed
    assert_eq!(limiter.client_count(), 0);
}
```

### 6. Concurrency Tests

Test thread safety:

```rust
#[test]
fn test_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let config = FluxLimiterConfig::new(100.0, 50.0);
    let limiter = Arc::new(
        FluxLimiter::with_config(config, MonotonicClock::new()).unwrap()
    );

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let limiter = Arc::clone(&limiter);
            thread::spawn(move || {
                for j in 0..1000 {
                    let client_id = format!("client_{}_{}", i, j);
                    limiter.check_request(client_id).unwrap();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
```

### 7. Decision Metadata Tests

Verify decision metadata accuracy:

```rust
#[test]
fn test_retry_after_metadata() {
    let clock = TestClock::new(0.0);
    let config = FluxLimiterConfig::new(10.0, 0.0);
    let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

    // First request allowed
    limiter.check_request("client1").unwrap();

    // Second request denied
    let decision = limiter.check_request("client1").unwrap();
    assert!(!decision.allowed);

    // Verify retry_after is approximately 0.1 seconds
    let retry_after = decision.retry_after_seconds.unwrap();
    assert!((retry_after - 0.1).abs() < 0.001);
}

#[test]
fn test_remaining_capacity() {
    let clock = TestClock::new(0.0);
    let config = FluxLimiterConfig::new(10.0, 5.0);
    let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

    // First request
    let decision = limiter.check_request("client1").unwrap();
    assert!(decision.allowed);

    // Should have some remaining capacity
    assert!(decision.remaining_capacity.is_some());

    // Make more requests and verify capacity decreases
    for _ in 0..5 {
        limiter.check_request("client1").unwrap();
    }

    // Capacity should be depleted
    let decision = limiter.check_request("client1").unwrap();
    assert!(!decision.allowed);
}
```

## Performance Testing

Timing does not belong in a test. A test that measures wall-clock time, such
as asserting that a request takes under a microsecond, passes on a fast idle
machine and fails on a slow or loaded one, and the failure says nothing about
the code. The crate's own tests contain no timing assertions: `cargo test`
checks behaviour only, and every test runs to the same result whatever the
machine is doing.

Timing is measured with [Criterion](https://github.com/bheisler/criterion.rs)
benchmarks, which live in `benches/` and are not part of `cargo test`:

```sh
cargo bench                        # all benchmarks
cargo bench contention             # only the multi-threaded ones
cargo bench -- --save-baseline before
# ... make a change ...
cargo bench -- --baseline before   # compare against the saved run
```

| Benchmark | What it measures |
|---|---|
| `single_client_u64` | One thread, one client, `u64` key |
| `single_client_string_clone` | One thread, one client, `String` key cloned per request |
| `contended_4_threads_u64` | 4 threads on one client, per batch |
| `contention_hot_key_u64` | 4 threads on one client, per request |
| `contention_spread_keys_u64` | 4 threads, each with its own clients, per request |

A test that needs to prove something about *time* rather than *speed* does it
with `TestClock`, which never waits: it sets the clock to an exact value, so a
test of "what happens after 9.9 seconds" runs instantly and always gives the
same answer.

Throughput figures from a benchmark are specific to the machine that produced
them. The numbers quoted in [Performance Design](./performance.md) were taken
on one laptop and are there to show the relative cost of the two contention
cases, not to promise a particular rate.

## Test Utilities

### Helper Functions

```rust
fn assert_allowed(result: Result<FluxLimiterDecision, FluxLimiterError>) {
    match result {
        Ok(decision) => assert!(decision.allowed, "Expected request to be allowed"),
        Err(e) => panic!("Expected allowed decision, got error: {:?}", e),
    }
}

fn assert_denied(result: Result<FluxLimiterDecision, FluxLimiterError>) {
    match result {
        Ok(decision) => assert!(!decision.allowed, "Expected request to be denied"),
        Err(e) => panic!("Expected denied decision, got error: {:?}", e),
    }
}

fn assert_error<T>(result: Result<T, FluxLimiterError>) {
    assert!(result.is_err(), "Expected error, got success");
}
```

### Test Fixtures

The suite builds limiters through two helpers, which always pair the limiter
with the `TestClock` it reads so a test can advance time:

```rust
/// A limiter with the given rate, burst and starting time, on a fresh TestClock.
fn setup_limiter(
    rate: f64,
    burst: f64,
    initial_time: f64,
) -> (FluxLimiter<String, TestClock>, TestClock>)

/// A limiter with the suite's standard configuration.
fn setup_standard_limiter() -> (FluxLimiter<String, TestClock>, TestClock)
```

A `TestClock` is created at time zero and can be moved forward by an amount
(`advance`) or set to an exact instant (`set_time` in seconds, `set_nanos` in
nanoseconds). It can also fail its next reading on request.

## Test Coverage

The suite is one integration test binary, `tests/ratelimiter`, split by
concern. It has 71 tests:

| File | Tests | Covers |
|---|---|---|
| `config_tests.rs` | 22 | Every rejected rate and burst, the bounds, and which error wins |
| `clock_tests.rs` | 13 | The clocks: monotonicity, clock steps, anchors, epoch accuracy |
| `error_tests.rs` | 8 | Clock failures and the errors they produce |
| `gcra_algorithm_tests.rs` | 8 | Allow and deny decisions against hand-computed sequences |
| `overflow_tests.rs` | 9 | Timestamps near `u64::MAX`, in both build profiles |
| `concurrency_tests.rs` | 5 | Many threads on one client, and that no update is lost |
| `cleanup_tests.rs` | 3 | Stale clients are removed and live ones are kept |
| `performance_tests.rs` | 2 | Behaviour at high request volumes, with no timing assertions |
| `decision_metadata_tests.rs` | 1 | The fields returned with a decision |

Performance itself is not tested here; see [Performance Testing](#performance-testing).

## Best Practices

1. **Use TestClock** for deterministic time control
2. **Test Error Paths** including clock failures
3. **Verify Metadata** not just allow/deny
4. **Test Concurrency** with multiple threads
5. **Measure Performance** with benchmarks
6. **Test Edge Cases** like zero burst, high rates
7. **Cleanup After Tests** to avoid state leakage

## Next Steps

- [Design Decisions](./design-decisions.md) - Understand the rationale
- [Future Extensibility](./future.md) - Planned enhancements