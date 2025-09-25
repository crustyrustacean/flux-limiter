# Flux Limiter

A high-performance rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free concurrent access.

## Features

- **Mathematically precise**: Implements the GCRA algorithm with exact nanosecond timing
- **High performance**: Lock-free concurrent access using DashMap
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Rich metadata**: Returns detailed decision information for HTTP response construction
- **Memory efficient**: Automatic cleanup of stale client entries
- **Robust error handling**: Graceful handling of clock failures and configuration errors
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
match limiter.check_request("user_123") {
    Ok(decision) => {
        if decision.allowed {
            println!("Request allowed");
        } else {
            println!("Rate limited - retry after {:.2}s", 
                     decision.retry_after_seconds.unwrap_or(0.0));
        }
    }
    Err(e) => {
        eprintln!("Rate limiter error: {}", e);
        // Handle error appropriately (e.g., allow request, log error)
    }
}
```

## Rate Limiting Decisions

The `check_request()` method returns a `Result<FluxLimiterDecision, FluxLimiterError>` with rich metadata:

```rust
pub struct FluxLimiterDecision {
    pub allowed: bool,                    // Whether to allow the request
    pub retry_after_seconds: Option<f64>, // When to retry (if denied)
    pub remaining_capacity: Option<f64>,  // Remaining burst capacity
    pub reset_time_nanos: u64,           // When the window resets
}
```

## Error Handling

Flux Limiter provides comprehensive error handling for robust production usage:

```rust
use flux_limiter::FluxLimiterError;

match limiter.check_request("client_id") {
    Ok(decision) => {
        // Handle rate limiting decision
        if decision.allowed {
            // Process request
        } else {
            // Rate limited - return 429
        }
    }
    Err(FluxLimiterError::ClockError(_)) => {
        // System clock issue - log error and decide policy
        // Common fallback: allow request or return 500
        eprintln!("Clock error in rate limiter");
    }
    Err(e) => {
        // Other configuration errors (shouldn't happen at runtime)
        eprintln!("Rate limiter configuration error: {}", e);
    }
}
```

### Error Types

- **`FluxLimiterError::InvalidRate`**: Rate must be positive (configuration error)
- **`FluxLimiterError::InvalidBurst`**: Burst must be non-negative (configuration error)  
- **`FluxLimiterError::ClockError`**: System time unavailable or inconsistent

### Error Handling Strategies

**For clock errors in production:**
- **Fail-open**: Allow requests when rate limiter fails
- **Fail-closed**: Deny requests when rate limiter fails
- **Fallback**: Use alternative rate limiting (e.g., in-memory counter)

```rust
let fallback_decision = match limiter.check_request(client_id) {
    Ok(decision) => decision.allowed,
    Err(FluxLimiterError::ClockError(_)) => {
        // Implement your policy: fail-open, fail-closed, or fallback
        true // Example: fail-open (allow request)
    }
    Err(_) => false, // Configuration errors should not happen at runtime
};
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
match limiter.check_request(client_ip) {
    Ok(decision) if decision.allowed => {
        // Process request
    }
    Ok(_) => {
        // Rate limited
    }
    Err(e) => {
        // Handle error
        eprintln!("Rate limiter error: {}", e);
    }
}
```

### Memory Management

```rust
// Clean up clients that haven't been seen for 1 hour
let one_hour_nanos = 60 * 60 * 1_000_000_000u64;
if let Err(e) = limiter.cleanup_stale_clients(one_hour_nanos) {
    eprintln!("Cleanup failed: {}", e);
    // Cleanup failure is usually not critical - log and continue
}

// Or clean up based on rate interval
let threshold = limiter.rate() as u64 * 100 * 1_000_000_000; // 100x rate interval
let _ = limiter.cleanup_stale_clients(threshold); // Ignore cleanup errors
```

## Web Framework Integration

### Example with Axum

```rust
use axum::{http::{StatusCode, HeaderMap}, response::Response};
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock, FluxLimiterError};
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
        Err(FluxLimiterError::ClockError(_)) => {
            // Handle clock errors - implement your policy here
            // This example uses fail-open (allow request)
            eprintln!("Rate limiter clock error - allowing request");
            Ok(Response::builder()
                .status(200)
                .body("Request processed (limiter degraded)".into())
                .unwrap())
        }
        Err(e) => {
            // Handle other errors (configuration issues)
            eprintln!("Rate limiter error: {}", e);
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
- **Reliability**: Graceful degradation on system clock issues

## Cleanup Recommendations

Call `cleanup_stale_clients()` periodically to prevent memory growth:

```rust
// In a background task
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(3600)); // 1 hour
    loop {
        interval.tick().await;
        let cleanup_threshold = 24 * 60 * 60 * 1_000_000_000u64; // 24 hours
        
        // Cleanup errors are typically not critical
        if let Err(e) = limiter.cleanup_stale_clients(cleanup_threshold) {
            eprintln!("Rate limiter cleanup failed: {}", e);
            // Consider implementing fallback cleanup or alerting
        }
    }
});
```

## Configuration Validation

```rust
use flux_limiter::{FluxLimiterConfig, FluxLimiterError};

match FluxLimiterConfig::new(10.0, 5.0).validate() {
    Ok(_) => println!("Valid configuration"),
    Err(FluxLimiterError::InvalidRate) => println!("Rate must be positive"),
    Err(FluxLimiterError::InvalidBurst) => println!("Burst must be non-negative"),
    Err(e) => println!("Configuration error: {}", e),
}

// Or use with_config which validates automatically
match FluxLimiter::with_config(config, SystemClock) {
    Ok(limiter) => {
        // Use limiter
    }
    Err(e) => {
        eprintln!("Failed to create rate limiter: {}", e);
        // Handle configuration error
    }
}
```

## Production Considerations

### Monitoring and Alerting

```rust
// Example: Count clock errors for monitoring
use std::sync::atomic::{AtomicU64, Ordering};

static CLOCK_ERROR_COUNT: AtomicU64 = AtomicU64::new(0);

match limiter.check_request(client_id) {
    Ok(decision) => decision.allowed,
    Err(FluxLimiterError::ClockError(_)) => {
        CLOCK_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
        // Log for monitoring/alerting
        // Implement your fallback policy
        true // Example: fail-open
    }
    Err(e) => {
        // Log configuration errors
        false
    }
}
```

### Graceful Degradation

Consider implementing circuit breaker patterns for persistent clock failures:

```rust
// Example: Skip rate limiting after consecutive failures
if consecutive_clock_failures > threshold {
    // Temporarily bypass rate limiting
    // Reset counter after successful operations
}
```

## License

This project is licensed under the MIT License - see the [License.txt](License.txt) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.