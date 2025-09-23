# Flux Limiter

A high-performance rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free concurrent access.

## Features

- **Mathematically precise**: Implements the GCRA algorithm with exact nanosecond timing
- **High performance**: Lock-free concurrent access using DashMap
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Rich metadata**: Returns detailed decision information for HTTP response construction
- **Memory efficient**: Automatic cleanup of stale client entries
- **Testable**: Clock abstraction enables deterministic testing
- **Thread-safe**: Safe to use across multiple threads
- **Zero allocations**: Efficient hot path with minimal overhead

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
flux-limiter = "0.4.0"
```

## Quick Start

```rust
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};

// Create a rate limiter: 10 requests per second with burst of 5
let config = FluxLimiterConfig::new(10.0, 5.0);
let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

// Check if a request should be allowed
let decision = limiter.check_request("user_123").unwrap();
if decision.allowed {
    println!("Request allowed");
} else {
    println!("Rate limited - retry after {:.2}s", 
             decision.retry_after_seconds.unwrap_or(0.0));
}
```

## Rate Limiting Decisions

The `check_request()` method returns a `FluxLimiterDecision` with rich metadata:

```rust
pub struct FluxLimiterDecision {
    pub allowed: bool,                    // Whether to allow the request
    pub retry_after_seconds: Option<f64>, // When to retry (if denied)
    pub remaining_capacity: Option<f64>,  // Remaining burst capacity
    pub reset_time_nanos: u64,           // When the window resets
}
```

## Configuration

### Builder Pattern

```rust
use flux_limiter::FluxLimiterConfig;

let config = FluxLimiterConfig::new(0.0, 0.0)
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
let config = FluxLimiterConfig::new(5.0, 10.0);
let limiter = FluxLimiter::<IpAddr, _>::with_config(config, SystemClock).unwrap();

let client_ip: IpAddr = "192.168.1.1".parse().unwrap();
let decision = limiter.check_request(client_ip).unwrap();
if decision.allowed {
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

## Web Framework Integration

### Example with Axum

```rust
use axum::{http::{StatusCode, HeaderMap}, response::Response};
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};
use std::sync::Arc;

async fn rate_limit_middleware(
    request: axum::extract::Request,
    limiter: Arc<FluxLimiter<String, SystemClock>>,
) -> Result<Response, (StatusCode, HeaderMap, &'static str)> {
    let client_ip = extract_client_ip(&request);
    
    match limiter.check_request(client_ip) {
        Ok(decision) if decision.allowed => {
            // Add rate limit headers to successful responses
            let mut headers = HeaderMap::new();
            if let Some(remaining) = decision.remaining_capacity {
                headers.insert("X-RateLimit-Remaining", 
                    remaining.to_string().parse().unwrap());
            }
            Ok(Response::builder()
                .status(200)
                .headers(headers)
                .body("Request processed".into())
                .unwrap())
        }
        Ok(decision) => {
            // Rate limited - return 429 with metadata
            let mut headers = HeaderMap::new();
            if let Some(retry_after) = decision.retry_after_seconds {
                headers.insert("Retry-After", 
                    (retry_after.ceil() as u64).to_string().parse().unwrap());
            }
            headers.insert("X-RateLimit-Remaining", "0".parse().unwrap());
            
            Err((StatusCode::TOO_MANY_REQUESTS, headers, "Rate limited"))
        }
        Err(_) => {
            // Handle limiter errors
            Err((StatusCode::INTERNAL_SERVER_ERROR, HeaderMap::new(), "Internal error"))
        }
    }
}
```

### Standard Rate Limit Headers

Flux Limiter provides all the metadata needed for standard HTTP rate limiting headers:

- **X-RateLimit-Remaining**: Use `decision.remaining_capacity`
- **Retry-After**: Use `decision.retry_after_seconds` (when denied)
- **X-RateLimit-Reset**: Convert `decision.reset_time_nanos` to timestamp

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
- **Time complexity**: O(1) for `check_request()` operations
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
use flux_limiter::{FluxLimiterConfig, FluxLimiterError};

match FluxLimiterConfig::new(0.0, 5.0).validate() {
    Ok(_) => println!("Valid configuration"),
    Err(FluxLimiterError::InvalidRate) => println!("Rate must be positive"),
    Err(FluxLimiterError::InvalidBurst) => println!("Burst must be non-negative"),
}
```

## License

This project is licensed under the MIT License - see the [License.txt](License.txt) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.