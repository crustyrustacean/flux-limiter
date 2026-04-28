# Basic Usage

Common patterns and best practices for using Flux Limiter.

## Simple Rate Limiting

The most basic use case - check if a request should be allowed:

```rust
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};

let config = FluxLimiterConfig::new(10.0, 5.0);
let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

match limiter.check_request("user_123") {
    Ok(decision) if decision.allowed => {
        // Process request
        println!("Request allowed");
    }
    Ok(decision) => {
        // Rate limited
        println!("Rate limited - retry after {:.2}s",
                 decision.retry_after_seconds.unwrap_or(0.0));
    }
    Err(e) => {
        // Handle error
        eprintln!("Error: {}", e);
    }
}
```

## Working with Different Client ID Types

### String Client IDs

```rust
// User IDs, session IDs, API keys
let limiter = FluxLimiter::<String, _>::with_config(config, SystemClock).unwrap();

limiter.check_request("user_123".to_string())?;
limiter.check_request("session_abc".to_string())?;
```

### IP Address Client IDs

```rust
use std::net::IpAddr;

let limiter = FluxLimiter::<IpAddr, _>::with_config(config, SystemClock).unwrap();

let client_ip: IpAddr = "192.168.1.1".parse().unwrap();
limiter.check_request(client_ip)?;
```

### Numeric Client IDs

```rust
// High-performance numeric IDs
let limiter = FluxLimiter::<u64, _>::with_config(config, SystemClock).unwrap();

limiter.check_request(12345)?;
limiter.check_request(67890)?;
```

### Custom Client IDs

```rust
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash)]
struct ClientId {
    user_id: u64,
    endpoint: String,
}

let limiter = FluxLimiter::<ClientId, _>::with_config(config, SystemClock).unwrap();

let client = ClientId {
    user_id: 123,
    endpoint: "/api/data".to_string(),
};

limiter.check_request(client)?;
```

## Accessing Configuration

You can query the current rate limiter configuration:

```rust
let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

println!("Rate: {} req/sec", limiter.rate());
println!("Burst: {}", limiter.burst());
```

## Using Decision Metadata

### Extracting All Metadata

```rust
match limiter.check_request("user_123") {
    Ok(decision) => {
        println!("Allowed: {}", decision.allowed);

        if let Some(retry_after) = decision.retry_after_seconds {
            println!("Retry after: {:.2}s", retry_after);
        }

        if let Some(remaining) = decision.remaining_capacity {
            println!("Remaining capacity: {:.2}", remaining);
        }

        println!("Reset time: {} ns", decision.reset_time_nanos);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Computing Reset Time

```rust
use std::time::{SystemTime, UNIX_EPOCH, Duration};

match limiter.check_request("user_123") {
    Ok(decision) => {
        // Convert reset_time_nanos to a timestamp
        let reset_duration = Duration::from_nanos(decision.reset_time_nanos);
        let reset_time = UNIX_EPOCH + reset_duration;

        let now = SystemTime::now();
        if let Ok(time_until_reset) = reset_time.duration_since(now) {
            println!("Reset in {:.2}s", time_until_reset.as_secs_f64());
        }
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## Sharing Across Threads

Flux Limiter is thread-safe and designed to be shared:

### Using Arc

```rust
use std::sync::Arc;
use std::thread;

let config = FluxLimiterConfig::new(100.0, 50.0);
let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

// Share across multiple threads
let handles: Vec<_> = (0..10)
    .map(|i| {
        let limiter = Arc::clone(&limiter);
        thread::spawn(move || {
            let client_id = format!("client_{}", i);
            limiter.check_request(client_id)
        })
    })
    .collect();

for handle in handles {
    match handle.join().unwrap() {
        Ok(decision) => println!("Allowed: {}", decision.allowed),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### Using Clone

`FluxLimiter` implements `Clone` — cloning shares the same `DashMap` client
state, providing the same shared-state semantics without an outer `Arc`:

```rust
let config = FluxLimiterConfig::new(100.0, 50.0);
let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

// Clone directly — both share the same client state
let limiter1 = limiter.clone();
let limiter2 = limiter.clone();

let handles: Vec<_> = (0..10)
    .map(|i| {
        // Clone for each thread
        let limiter = limiter.clone();
        thread::spawn(move || {
            let client_id = format!("client_{}", i);
            limiter.check_request(client_id)
        })
    })
    .collect();
```

## Async Usage

While Flux Limiter is synchronous, it works seamlessly in async contexts:

```rust
use std::sync::Arc;
use tokio;

#[tokio::main]
async fn main() {
    let config = FluxLimiterConfig::new(100.0, 50.0);
    let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

    // Use in async tasks
    let limiter_clone = Arc::clone(&limiter);
    tokio::spawn(async move {
        match limiter_clone.check_request("user_123") {
            Ok(decision) if decision.allowed => {
                // Process async request
                println!("Processing request...");
            }
            Ok(_) => println!("Rate limited"),
            Err(e) => eprintln!("Error: {}", e),
        }
    })
    .await
    .unwrap();
}
```

## Multiple Rate Limiters

You can create different rate limiters for different purposes:

```rust
// Global rate limiter
let global_limiter = FluxLimiter::with_config(
    FluxLimiterConfig::new(1000.0, 500.0),
    SystemClock
).unwrap();

// Per-user rate limiter
let user_limiter = FluxLimiter::with_config(
    FluxLimiterConfig::new(10.0, 5.0),
    SystemClock
).unwrap();

// Apply both limits
fn check_both_limits(user_id: &str) -> bool {
    match global_limiter.check_request("global") {
        Ok(decision) if !decision.allowed => return false,
        Err(_) => return false,
        _ => {}
    }

    match user_limiter.check_request(user_id) {
        Ok(decision) => decision.allowed,
        Err(_) => false,
    }
}
```

## Idiomatic Error Handling

```rust
use flux_limiter::{FluxLimiter, FluxLimiterError};

fn process_request(limiter: &FluxLimiter<String, SystemClock>, client_id: String) -> Result<(), String> {
    let decision = limiter
        .check_request(client_id)
        .map_err(|e| format!("Rate limiter error: {}", e))?;

    if !decision.allowed {
        return Err(format!(
            "Rate limited. Retry after {:.2}s",
            decision.retry_after_seconds.unwrap_or(0.0)
        ));
    }

    // Process the request
    Ok(())
}
```

## Next Steps

- [Error Handling](./error-handling.md) - Handle errors gracefully
- [Advanced Usage](./advanced-usage.md) - Memory management and optimization
- [Web Integration](./web-integration.md) - Use with web frameworks
