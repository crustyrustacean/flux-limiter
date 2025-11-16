# Quick Start

Get started with Flux Limiter in just a few minutes.

## Basic Example

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

## Understanding the Example

Let's break down what's happening:

1. **Create Configuration**: `FluxLimiterConfig::new(10.0, 5.0)`
   - Rate: 10 requests per second
   - Burst: 5 additional requests allowed in bursts
   - Total capacity: ~6 requests can be made immediately

2. **Create Rate Limiter**: `FluxLimiter::with_config(config, SystemClock)`
   - Uses the configuration
   - Uses `SystemClock` for production time source
   - Returns `Result` to handle configuration errors

3. **Check Request**: `limiter.check_request("user_123")`
   - Checks if the client "user_123" can make a request
   - Returns rich metadata about the decision
   - Automatically updates internal state

## Decision Metadata

The `FluxLimiterDecision` struct provides detailed information:

```rust
pub struct FluxLimiterDecision {
    pub allowed: bool,                    // Whether to allow the request
    pub retry_after_seconds: Option<f64>, // When to retry (if denied)
    pub remaining_capacity: Option<f64>,  // Remaining burst capacity
    pub reset_time_nanos: u64,           // When the window resets
}
```

### Using Decision Metadata

```rust
match limiter.check_request("user_123") {
    Ok(decision) => {
        if decision.allowed {
            println!("Request allowed");
            if let Some(remaining) = decision.remaining_capacity {
                println!("Remaining capacity: {:.2}", remaining);
            }
        } else {
            if let Some(retry_after) = decision.retry_after_seconds {
                println!("Please retry after {:.2} seconds", retry_after);
            }
        }
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

## Multiple Clients

Flux Limiter automatically tracks state for each unique client:

```rust
let config = FluxLimiterConfig::new(10.0, 5.0);
let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

// Different clients have independent rate limits
limiter.check_request("user_1").unwrap();
limiter.check_request("user_2").unwrap();
limiter.check_request("user_3").unwrap();

// Each client is tracked separately
```

## Thread Safety

Flux Limiter is thread-safe and can be shared across threads:

```rust
use std::sync::Arc;
use std::thread;

let config = FluxLimiterConfig::new(10.0, 5.0);
let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

let mut handles = vec![];

for i in 0..10 {
    let limiter = Arc::clone(&limiter);
    let handle = thread::spawn(move || {
        let client_id = format!("user_{}", i);
        limiter.check_request(client_id)
    });
    handles.push(handle);
}

for handle in handles {
    let result = handle.join().unwrap();
    println!("Result: {:?}", result);
}
```

## Next Steps

- [Configuration](./configuration.md) - Learn about rate and burst configuration
- [Basic Usage](../guide/basic-usage.md) - Explore common usage patterns
- [Error Handling](../guide/error-handling.md) - Handle errors gracefully
