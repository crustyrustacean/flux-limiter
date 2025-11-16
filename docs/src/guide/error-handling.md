# Error Handling

Comprehensive guide to handling errors in Flux Limiter.

## Error Types

Flux Limiter provides a well-defined error hierarchy:

```rust
pub enum FluxLimiterError {
    InvalidRate,           // Configuration: rate ≤ 0
    InvalidBurst,          // Configuration: burst < 0
    ClockError(ClockError), // Runtime: clock failure
}

pub enum ClockError {
    SystemTimeError,       // System time unavailable
}
```

## Configuration Errors

Configuration errors occur when creating a rate limiter with invalid settings.

### InvalidRate Error

```rust
use flux_limiter::{FluxLimiterConfig, FluxLimiter, SystemClock, FluxLimiterError};

// Invalid: rate must be positive
let config = FluxLimiterConfig::new(-10.0, 5.0);

match FluxLimiter::with_config(config, SystemClock) {
    Ok(_) => println!("Success"),
    Err(FluxLimiterError::InvalidRate) => {
        eprintln!("Error: Rate must be positive (> 0)");
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

### InvalidBurst Error

```rust
// Invalid: burst must be non-negative
let config = FluxLimiterConfig::new(10.0, -5.0);

match FluxLimiter::with_config(config, SystemClock) {
    Ok(_) => println!("Success"),
    Err(FluxLimiterError::InvalidBurst) => {
        eprintln!("Error: Burst must be non-negative (≥ 0)");
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

### Handling Configuration Errors

Configuration errors should be caught early, typically at application startup:

```rust
fn create_rate_limiter() -> Result<FluxLimiter<String, SystemClock>, String> {
    let config = FluxLimiterConfig::new(100.0, 50.0);

    FluxLimiter::with_config(config, SystemClock)
        .map_err(|e| match e {
            FluxLimiterError::InvalidRate => {
                "Invalid configuration: rate must be positive".to_string()
            }
            FluxLimiterError::InvalidBurst => {
                "Invalid configuration: burst must be non-negative".to_string()
            }
            _ => format!("Configuration error: {}", e),
        })
}

fn main() {
    let limiter = create_rate_limiter()
        .expect("Failed to create rate limiter with valid configuration");

    // Use limiter...
}
```

## Runtime Clock Errors

Clock errors can occur during normal operation when the system clock is unavailable or behaves unexpectedly.

### Understanding Clock Errors

Clock errors happen when:
- System time API fails
- Clock jumps backward (NTP adjustment)
- System suspend/resume causes time discontinuity
- Virtualization causes time skips

### Basic Clock Error Handling

```rust
match limiter.check_request("user_123") {
    Ok(decision) => {
        if decision.allowed {
            // Process request
        } else {
            // Rate limited
        }
    }
    Err(FluxLimiterError::ClockError(_)) => {
        eprintln!("System clock error detected");
        // Implement your error policy
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

## Error Handling Policies

Different applications require different error handling strategies.

### Fail-Open Policy

Allow requests when the rate limiter encounters errors:

```rust
fn should_allow_request(
    limiter: &FluxLimiter<String, SystemClock>,
    client_id: &str
) -> bool {
    match limiter.check_request(client_id) {
        Ok(decision) => decision.allowed,
        Err(FluxLimiterError::ClockError(_)) => {
            // Fail-open: allow request on clock error
            eprintln!("Clock error - allowing request (fail-open policy)");
            true
        }
        Err(e) => {
            eprintln!("Rate limiter error: {} - allowing request", e);
            true
        }
    }
}
```

**Use when:**
- Availability is more important than strict rate limiting
- False positives (allowing too many requests) are acceptable
- Your backend can handle temporary spikes

### Fail-Closed Policy

Deny requests when the rate limiter encounters errors:

```rust
fn should_allow_request(
    limiter: &FluxLimiter<String, SystemClock>,
    client_id: &str
) -> bool {
    match limiter.check_request(client_id) {
        Ok(decision) => decision.allowed,
        Err(FluxLimiterError::ClockError(_)) => {
            // Fail-closed: deny request on clock error
            eprintln!("Clock error - denying request (fail-closed policy)");
            false
        }
        Err(e) => {
            eprintln!("Rate limiter error: {} - denying request", e);
            false
        }
    }
}
```

**Use when:**
- Security is paramount
- False negatives (denying legitimate requests) are acceptable
- Protecting backend from overload is critical

### Fallback Policy

Use alternative rate limiting when clock errors occur:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct FallbackRateLimiter {
    primary: FluxLimiter<String, SystemClock>,
    fallback_counter: Arc<AtomicU64>,
    fallback_limit: u64,
}

impl FallbackRateLimiter {
    fn check_request(&self, client_id: String) -> bool {
        match self.primary.check_request(client_id) {
            Ok(decision) => decision.allowed,
            Err(FluxLimiterError::ClockError(_)) => {
                // Use simple counter as fallback
                let count = self.fallback_counter.fetch_add(1, Ordering::Relaxed);

                if count >= self.fallback_limit {
                    eprintln!("Fallback limit reached");
                    false
                } else {
                    eprintln!("Using fallback counter: {}/{}", count, self.fallback_limit);
                    true
                }
            }
            Err(e) => {
                eprintln!("Unexpected error: {}", e);
                false
            }
        }
    }
}
```

## Monitoring Clock Errors

Track clock errors for alerting and debugging:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static CLOCK_ERROR_COUNT: AtomicU64 = AtomicU64::new(0);
static TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);

fn check_with_monitoring(
    limiter: &FluxLimiter<String, SystemClock>,
    client_id: String
) -> bool {
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);

    match limiter.check_request(client_id) {
        Ok(decision) => decision.allowed,
        Err(FluxLimiterError::ClockError(e)) => {
            CLOCK_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);

            let error_count = CLOCK_ERROR_COUNT.load(Ordering::Relaxed);
            let total = TOTAL_REQUESTS.load(Ordering::Relaxed);
            let error_rate = error_count as f64 / total as f64;

            eprintln!("Clock error: {:?} (rate: {:.4}%)", e, error_rate * 100.0);

            // Implement your policy
            true // Fail-open
        }
        Err(e) => {
            eprintln!("Unexpected error: {}", e);
            false
        }
    }
}
```

## Circuit Breaker Pattern

Temporarily bypass rate limiting after consecutive failures:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

struct CircuitBreakerLimiter {
    limiter: FluxLimiter<String, SystemClock>,
    consecutive_failures: AtomicU64,
    failure_threshold: u64,
    bypassed: AtomicU64,
}

impl CircuitBreakerLimiter {
    fn check_request(&self, client_id: String) -> bool {
        // Check if circuit is open
        if self.consecutive_failures.load(Ordering::Relaxed) >= self.failure_threshold {
            self.bypassed.fetch_add(1, Ordering::Relaxed);
            eprintln!("Circuit open - bypassing rate limiter");
            return true;
        }

        match self.limiter.check_request(client_id) {
            Ok(decision) => {
                // Reset failure counter on success
                self.consecutive_failures.store(0, Ordering::Relaxed);
                decision.allowed
            }
            Err(FluxLimiterError::ClockError(_)) => {
                let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;

                if failures >= self.failure_threshold {
                    eprintln!("Opening circuit after {} consecutive failures", failures);
                }

                true // Fail-open
            }
            Err(e) => {
                eprintln!("Unexpected error: {}", e);
                false
            }
        }
    }
}
```

## Cleanup Error Handling

The `cleanup_stale_clients` method can also return clock errors:

```rust
// Cleanup errors are typically not critical
match limiter.cleanup_stale_clients(one_hour_nanos) {
    Ok(count) => {
        println!("Cleaned up {} stale clients", count);
    }
    Err(FluxLimiterError::ClockError(_)) => {
        eprintln!("Clock error during cleanup - will retry later");
        // Cleanup failure is not critical - continue operation
    }
    Err(e) => {
        eprintln!("Unexpected cleanup error: {}", e);
    }
}
```

## Best Practices

1. **Validate Configuration Early**: Check configuration at startup, not runtime
2. **Choose an Error Policy**: Decide on fail-open, fail-closed, or fallback
3. **Monitor Errors**: Track error rates for alerting
4. **Log Contextually**: Include client ID and error context in logs
5. **Handle Gracefully**: Never panic - always return a decision
6. **Test Error Paths**: Use TestClock to simulate failures
7. **Document Policy**: Make your error handling policy explicit

## Next Steps

- [Advanced Usage](./advanced-usage.md) - Memory management and optimization
- [Production Considerations](./production.md) - Deploy with confidence
- [Testing Architecture](../architecture/testing.md) - Test error handling
