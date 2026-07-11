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
    let result = FluxLimiter::with_config(config, SystemClock);

    assert!(matches!(result, Err(FluxLimiterError::InvalidRate)));
}

#[test]
fn test_invalid_burst() {
    let config = FluxLimiterConfig::new(10.0, -5.0);
    let result = FluxLimiter::with_config(config, SystemClock);

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
        FluxLimiter::with_config(config, SystemClock).unwrap()
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

### Latency Benchmarks

```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_check_request_latency() {
        let limiter = FluxLimiter::with_config(
            FluxLimiterConfig::new(1000.0, 500.0),
            SystemClock,
        ).unwrap();

        let iterations = 100_000;
        let start = Instant::now();

        for i in 0..iterations {
            let client_id = format!("client_{}", i % 1000);
            limiter.check_request(client_id).unwrap();
        }

        let elapsed = start.elapsed();
        let avg_latency = elapsed.as_nanos() / iterations;

        println!("Average latency: {}ns", avg_latency);
        assert!(avg_latency < 1000); // Should be under 1μs
    }
}
```

### Throughput Tests

```rust
#[test]
fn test_throughput() {
    let limiter = Arc::new(
        FluxLimiter::with_config(
            FluxLimiterConfig::new(10_000.0, 5_000.0),
            SystemClock,
        ).unwrap()
    );

    let start = Instant::now();
    let threads = 8;
    let requests_per_thread = 100_000;

    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let limiter = Arc::clone(&limiter);
            thread::spawn(move || {
                for i in 0..requests_per_thread {
                    let client_id = format!("client_{}_{}", t, i % 1000);
                    limiter.check_request(client_id).unwrap();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    let total_requests = threads * requests_per_thread;
    let throughput = total_requests as f64 / elapsed.as_secs_f64();

    println!("Throughput: {:.2} req/s", throughput);
}
```

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

```rust
fn create_test_limiter(rate: f64, burst: f64) -> (FluxLimiter<String, TestClock>, TestClock) {
    let clock = TestClock::new(0.0);
    let config = FluxLimiterConfig::new(rate, burst);
    let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();
    (limiter, clock)
}
```

## Test Coverage

Aim for comprehensive coverage:

- ✅ Algorithm correctness
- ✅ Configuration validation
- ✅ Error handling and recovery
- ✅ Concurrency safety
- ✅ Memory management
- ✅ Decision metadata accuracy
- ✅ Performance characteristics
- ✅ Edge cases and boundary conditions

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