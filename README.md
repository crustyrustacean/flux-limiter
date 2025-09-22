# Flux Limiter

A high-performance rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free concurrent access.

## Features

- **Mathematically precise**: Implements the GCRA algorithm with exact nanosecond timing
- **High performance**: Lock-free concurrent access using DashMap
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Memory efficient**: Automatic cleanup of stale client entries
- **Testable**: Clock abstraction enables deterministic testing
- **Thread-safe**: Safe to use across multiple threads
- **Zero allocations**: Efficient hot path with minimal overhead

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
flux-limiter = "0.3.0"
```

## Quick Start

```rust
use flux_limiter::{RateLimiter, RateLimiterConfig, SystemClock};

// Create a rate limiter: 10 requests per second with burst of 5
let config = RateLimiterConfig::new(10.0, 5.0);
let limiter = RateLimiter::with_config(config, SystemClock).unwrap();

// Check if a request should be allowed
match limiter.is_allowed("user_123") {
    Ok(true) => println!("Request allowed"),
    Ok(false) => println!("Rate limited - deny request"),
    Err(e) => println!("Error: {}", e),
}
```

## Configuration

### Builder Pattern

```rust
use flux_limiter::RateLimiterConfig;

let config = RateLimiterConfig::new(0.0, 0.0)
    .rate(100.0)        // 100 requests per second
    .burst(50.0);       // Allow bursts of up to 50 requests
```

### Rate and Burst Explained

- **Rate**: Sustained requests per second (must be > 0)
- **Burst**: Additional requests allowed in short bursts (must be ≥ 0)
- **Total capacity**: Approximately `1 + burst` requests can be made immediately

Example: With `rate=10.0` and `burst=5.0`:
- Sustained rate: 10 requests per second (one every 100ms)
- Burst allowance: ~6 requests can be made immediately
- After burst: Limited to 10 req/sec sustained rate

## Advanced Usage

### Custom Client ID Types

```rust
use std::net::IpAddr;

// Use IP addresses as client identifiers
let config = RateLimiterConfig::new(5.0, 10.0);
let limiter = RateLimiter::<IpAddr, _>::with_config(config, SystemClock).unwrap();

let client_ip: IpAddr = "192.168.1.1".parse().unwrap();
if limiter.is_allowed(client_ip).unwrap() {
    // Process request
}
```

### Memory Management

```rust
// Clean up clients that haven't been seen for 1 hour
let one_hour_nanos = 60 * 60 * 1_000_000_000u64;
limiter.cleanup_stale_clients(one_hour_nanos);

// Or clean up based on rate interval
let threshold = limiter.rate() as u64 * 100 * 1_000_000_000; // 100x rate interval
limiter.cleanup_stale_clients(threshold);
```

### Testing Support

Enable the `testing` feature for deterministic testing:

```toml
[dependencies]
flux-limiter = { version = "0.3.0", features = ["testing"] }
```

```rust
#[cfg(test)]
mod tests {
    use flux_limiter::{RateLimiter, RateLimiterConfig, TestClock};

    #[test]
    fn test_rate_limiting() {
        let clock = TestClock::new(0.0);
        let config = RateLimiterConfig::new(1.0, 0.0); // 1 req/sec, no burst
        let limiter = RateLimiter::with_config(config, clock.clone()).unwrap();

        // First request allowed
        assert!(limiter.is_allowed("client").unwrap());
        
        // Second request blocked
        assert!(!limiter.is_allowed("client").unwrap());
        
        // Advance time by 1 second
        clock.advance(1.0);
        
        // Request allowed again
        assert!(limiter.is_allowed("client").unwrap());
    }
}
```

## Web Framework Integration

### Example with Axum

```rust
use axum::{http::StatusCode, response::Response};
use flux_limiter::{RateLimiter, RateLimiterConfig, SystemClock};
use std::sync::Arc;

async fn rate_limit_middleware(
    request: axum::extract::Request,
    limiter: Arc<RateLimiter<String, SystemClock>>,
) -> Result<Response, StatusCode> {
    let client_ip = extract_client_ip(&request);
    
    match limiter.is_allowed(client_ip) {
        Ok(true) => {
            // Process request normally
            Ok(Response::new("Request processed".into()))
        }
        Ok(false) => {
            // Return 429 Too Many Requests
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
        Err(_) => {
            // Handle limiter errors
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

## Algorithm Details

Flux Limiter implements the Generic Cell Rate Algorithm (GCRA), which is mathematically equivalent to the token bucket algorithm but uses a different approach:

- **Token Bucket**: Tracks available tokens, refills over time
- **GCRA**: Tracks theoretical arrival time of next request

GCRA advantages:
- No background token refill processes
- Exact timing without floating-point precision loss
- Efficient state representation (one timestamp per client)

## Performance Characteristics

- **Memory**: O(number of active clients)
- **Time complexity**: O(1) for `is_allowed()` operations
- **Concurrency**: Lock-free reads and writes via DashMap
- **Precision**: Nanosecond timing accuracy
- **Throughput**: Millions of operations per second

## Cleanup Recommendations

Call `cleanup_stale_clients()` periodically to prevent memory growth:

```rust
// In a background task
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(3600)); // 1 hour
    loop {
        interval.tick().await;
        let cleanup_threshold = 24 * 60 * 60 * 1_000_000_000u64; // 24 hours
        limiter.cleanup_stale_clients(cleanup_threshold);
    }
});
```

## Error Handling

```rust
use flux_limiter::{RateLimiterConfig, RateLimiterError};

match RateLimiterConfig::new(0.0, 5.0).validate() {
    Ok(_) => println!("Valid configuration"),
    Err(RateLimiterError::InvalidRate) => println!("Rate must be positive"),
    Err(RateLimiterError::InvalidBurst) => println!("Burst must be non-negative"),
}
```

## License

This project is licensed under the MIT License - see the [License.txt](License.txt) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.