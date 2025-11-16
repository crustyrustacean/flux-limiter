# Error Handling Architecture

Comprehensive error handling design for production reliability.

## Error Type Hierarchy

```
FluxLimiterError
├── InvalidRate        (Configuration)
├── InvalidBurst       (Configuration)
└── ClockError         (Runtime)
    └── SystemTimeError
```

## Design Principles

### 1. Explicit Error Types

No silent failures - all errors are explicitly typed:

```rust
pub enum FluxLimiterError {
    InvalidRate,           // Rate ≤ 0
    InvalidBurst,          // Burst < 0
    ClockError(ClockError), // System time failure
}
```

### 2. Configuration vs Runtime Errors

**Configuration Errors** (at startup):
- `InvalidRate`: Caught during limiter creation
- `InvalidBurst`: Caught during limiter creation
- Should never occur in production after startup

**Runtime Errors** (during operation):
- `ClockError`: Can occur anytime
- Requires graceful handling
- Application chooses policy

### 3. Graceful Degradation

Applications can choose error handling policy:
- **Fail-open**: Allow requests on errors
- **Fail-closed**: Deny requests on errors
- **Fallback**: Use alternative rate limiting

## Configuration Error Handling

### Early Validation

Validate configuration at startup:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = FluxLimiterConfig::new(100.0, 50.0);

    let limiter = FluxLimiter::with_config(config, SystemClock)
        .map_err(|e| match e {
            FluxLimiterError::InvalidRate => {
                "Invalid rate: must be positive".to_string()
            }
            FluxLimiterError::InvalidBurst => {
                "Invalid burst: must be non-negative".to_string()
            }
            _ => format!("Configuration error: {}", e),
        })?;

    // Use limiter...
    Ok(())
}
```

### Explicit Validation

```rust
impl FluxLimiterConfig {
    pub fn validate(&self) -> Result<(), FluxLimiterError> {
        if self.rate_per_second <= 0.0 {
            return Err(FluxLimiterError::InvalidRate);
        }
        if self.burst_capacity < 0.0 {
            return Err(FluxLimiterError::InvalidBurst);
        }
        Ok(())
    }
}
```

**When to use**:
- Before storing configuration
- When loading from external sources
- Before creating rate limiter

## Runtime Clock Errors

### Clock Error Sources

Clock errors occur when:

1. **System Time Unavailable**
   - System clock not accessible
   - Permissions issues
   - Platform limitations

2. **Time Goes Backward**
   - NTP adjustments
   - Manual clock changes
   - Virtualization issues

3. **Time Discontinuities**
   - System suspend/resume
   - Hibernation
   - Container migrations

### Error Propagation

```
Clock::now() → Result<u64, ClockError>
     ↓
FluxLimiter::check_request() → Result<Decision, FluxLimiterError>
     ↓
Application Layer → Implements error policy
```

### Handling Clock Errors

#### Fail-Open Policy

Allow requests when clock fails:

```rust
match limiter.check_request(client_id) {
    Ok(decision) => decision.allowed,
    Err(FluxLimiterError::ClockError(_)) => {
        eprintln!("Clock error - allowing request (fail-open)");
        true
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
        true
    }
}
```

**Use when**:
- Availability > strict rate limiting
- False positives acceptable
- Backend can handle spikes

#### Fail-Closed Policy

Deny requests when clock fails:

```rust
match limiter.check_request(client_id) {
    Ok(decision) => decision.allowed,
    Err(FluxLimiterError::ClockError(_)) => {
        eprintln!("Clock error - denying request (fail-closed)");
        false
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
        false
    }
}
```

**Use when**:
- Security paramount
- False negatives acceptable
- Protecting backend critical

#### Fallback Policy

Use alternative when clock fails:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static FALLBACK_COUNTER: AtomicU64 = AtomicU64::new(0);
const FALLBACK_LIMIT: u64 = 1000;

match limiter.check_request(client_id) {
    Ok(decision) => decision.allowed,
    Err(FluxLimiterError::ClockError(_)) => {
        let count = FALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed);
        count < FALLBACK_LIMIT
    }
    Err(_) => false,
}
```

**Use when**:
- Need graceful degradation
- Have fallback mechanism
- Want best effort

## Error Monitoring

### Tracking Error Rates

```rust
use std::sync::atomic::{AtomicU64, Ordering};

struct ErrorMetrics {
    total_requests: AtomicU64,
    clock_errors: AtomicU64,
    config_errors: AtomicU64,
}

impl ErrorMetrics {
    fn record_result(&self, result: &Result<FluxLimiterDecision, FluxLimiterError>) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        if let Err(e) = result {
            match e {
                FluxLimiterError::ClockError(_) => {
                    self.clock_errors.fetch_add(1, Ordering::Relaxed);
                }
                FluxLimiterError::InvalidRate | FluxLimiterError::InvalidBurst => {
                    self.config_errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn error_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        let errors = self.clock_errors.load(Ordering::Relaxed);

        if total == 0 {
            0.0
        } else {
            errors as f64 / total as f64
        }
    }
}
```

### Alerting on Errors

```rust
fn check_with_alerting(
    limiter: &FluxLimiter<String, SystemClock>,
    metrics: &ErrorMetrics,
    client_id: String,
) -> bool {
    let result = limiter.check_request(client_id);
    metrics.record_result(&result);

    // Alert if error rate exceeds threshold
    if metrics.error_rate() > 0.01 {
        eprintln!(
            "ALERT: Clock error rate exceeded 1%: {:.2}%",
            metrics.error_rate() * 100.0
        );
    }

    match result {
        Ok(decision) => decision.allowed,
        Err(FluxLimiterError::ClockError(_)) => true, // Fail-open
        Err(_) => false,
    }
}
```

## Circuit Breaker Pattern

Temporarily bypass rate limiting after consecutive failures:

```rust
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

struct CircuitBreaker {
    consecutive_failures: AtomicU64,
    circuit_open: AtomicBool,
    threshold: u64,
}

impl CircuitBreaker {
    fn new(threshold: u64) -> Self {
        Self {
            consecutive_failures: AtomicU64::new(0),
            circuit_open: AtomicBool::new(false),
            threshold,
        }
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.circuit_open.store(false, Ordering::Relaxed);
    }

    fn record_failure(&self) -> bool {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;

        if failures >= self.threshold {
            if !self.circuit_open.swap(true, Ordering::Relaxed) {
                eprintln!("Circuit breaker opened after {} failures", failures);
            }
            true
        } else {
            false
        }
    }

    fn is_open(&self) -> bool {
        self.circuit_open.load(Ordering::Relaxed)
    }
}

fn check_with_circuit_breaker(
    limiter: &FluxLimiter<String, SystemClock>,
    breaker: &CircuitBreaker,
    client_id: String,
) -> bool {
    if breaker.is_open() {
        return true; // Bypass rate limiting
    }

    match limiter.check_request(client_id) {
        Ok(decision) => {
            breaker.record_success();
            decision.allowed
        }
        Err(FluxLimiterError::ClockError(_)) => {
            breaker.record_failure();
            true // Fail-open
        }
        Err(_) => false,
    }
}
```

## Cleanup Error Handling

Cleanup errors are typically non-critical:

```rust
match limiter.cleanup_stale_clients(threshold) {
    Ok(count) => {
        info!("Cleaned up {} stale clients", count);
    }
    Err(FluxLimiterError::ClockError(_)) => {
        warn!("Clock error during cleanup - will retry later");
        // Cleanup failure is not critical - continue operation
    }
    Err(e) => {
        error!("Unexpected cleanup error: {}", e);
    }
}
```

## Error Recovery Strategies

### 1. Retry with Backoff

```rust
async fn check_with_retry(
    limiter: &FluxLimiter<String, SystemClock>,
    client_id: String,
    max_retries: u32,
) -> Result<FluxLimiterDecision, FluxLimiterError> {
    let mut retries = 0;
    let mut delay = Duration::from_millis(10);

    loop {
        match limiter.check_request(client_id.clone()) {
            Ok(decision) => return Ok(decision),
            Err(FluxLimiterError::ClockError(_)) if retries < max_retries => {
                eprintln!("Clock error, retrying in {:?}", delay);
                tokio::time::sleep(delay).await;
                delay *= 2;
                retries += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

### 2. Degrade Gracefully

```rust
enum RateLimitStrategy {
    Primary(FluxLimiter<String, SystemClock>),
    Fallback(SimpleCounter),
}

impl RateLimitStrategy {
    fn check_request(&mut self, client_id: String) -> bool {
        match self {
            Self::Primary(limiter) => {
                match limiter.check_request(client_id) {
                    Ok(decision) => decision.allowed,
                    Err(FluxLimiterError::ClockError(_)) => {
                        // Switch to fallback
                        eprintln!("Switching to fallback strategy");
                        *self = Self::Fallback(SimpleCounter::new());
                        true
                    }
                    Err(_) => false,
                }
            }
            Self::Fallback(counter) => counter.check(),
        }
    }
}
```

### 3. Log and Continue

```rust
fn check_with_logging(
    limiter: &FluxLimiter<String, SystemClock>,
    client_id: String,
) -> bool {
    match limiter.check_request(client_id.clone()) {
        Ok(decision) => {
            trace!("Rate limit check succeeded for {}", client_id);
            decision.allowed
        }
        Err(FluxLimiterError::ClockError(e)) => {
            error!("Clock error for {}: {:?}", client_id, e);
            true // Fail-open
        }
        Err(e) => {
            error!("Unexpected error for {}: {:?}", client_id, e);
            false
        }
    }
}
```

## Testing Error Handling

### Simulating Clock Failures

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(
            limiter.check_request("client1"),
            Err(FluxLimiterError::ClockError(_))
        ));

        // Recovery
        assert!(limiter.check_request("client1").unwrap().allowed);
    }
}
```

## Best Practices

1. **Fail Fast on Configuration**: Validate at startup
2. **Choose Error Policy**: Fail-open, fail-closed, or fallback
3. **Monitor Errors**: Track error rates and alert
4. **Log Contextually**: Include client ID and error details
5. **Test Error Paths**: Use TestClock to simulate failures
6. **Document Policy**: Make error handling explicit

## Next Steps

- [Testing Architecture](./testing.md) - Test error handling
- [Production Considerations](../guide/production.md) - Deploy with confidence
- [Design Decisions](./design-decisions.md) - Why these choices?
