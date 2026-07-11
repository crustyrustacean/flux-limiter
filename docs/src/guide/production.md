# Production Considerations

Best practices for deploying Flux Limiter in production environments.

## Configuration

### Choose Appropriate Limits

Consider your service's capacity and SLAs:

```rust
// Calculate based on backend capacity
let backend_capacity = 10_000.0; // 10k requests/second
let safety_margin = 0.8; // 80% of capacity
let clients_count = 100.0; // Expected concurrent clients

let rate_per_client = (backend_capacity * safety_margin) / clients_count;
let config = FluxLimiterConfig::new(rate_per_client, rate_per_client * 0.5);
```

### Environment-Based Configuration

```rust
use std::env;

fn create_rate_limiter() -> Result<FluxLimiter<String, SystemClock>, FluxLimiterError> {
    let rate = env::var("RATE_LIMIT_RATE")
        .unwrap_or_else(|_| "100.0".to_string())
        .parse::<f64>()
        .expect("Invalid RATE_LIMIT_RATE");

    let burst = env::var("RATE_LIMIT_BURST")
        .unwrap_or_else(|_| "50.0".to_string())
        .parse::<f64>()
        .expect("Invalid RATE_LIMIT_BURST");

    let config = FluxLimiterConfig::new(rate, burst);
    FluxLimiter::with_config(config, SystemClock)
}
```

## Monitoring and Observability

### Metrics Collection

Track rate limiting decisions for monitoring:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

struct RateLimiterMetrics {
    total_requests: AtomicU64,
    allowed_requests: AtomicU64,
    denied_requests: AtomicU64,
    clock_errors: AtomicU64,
}

impl RateLimiterMetrics {
    fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            allowed_requests: AtomicU64::new(0),
            denied_requests: AtomicU64::new(0),
            clock_errors: AtomicU64::new(0),
        }
    }

    fn record_decision(&self, result: &Result<FluxLimiterDecision, FluxLimiterError>) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        match result {
            Ok(decision) if decision.allowed => {
                self.allowed_requests.fetch_add(1, Ordering::Relaxed);
            }
            Ok(_) => {
                self.denied_requests.fetch_add(1, Ordering::Relaxed);
            }
            Err(FluxLimiterError::ClockError(_)) => {
                self.clock_errors.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {}
        }
    }

    fn get_stats(&self) -> (u64, u64, u64, u64) {
        (
            self.total_requests.load(Ordering::Relaxed),
            self.allowed_requests.load(Ordering::Relaxed),
            self.denied_requests.load(Ordering::Relaxed),
            self.clock_errors.load(Ordering::Relaxed),
        )
    }
}
```

### Prometheus Integration

```rust
use prometheus::{Counter, IntCounter, Registry};
use std::sync::Arc;

struct PrometheusRateLimiter {
    limiter: Arc<FluxLimiter<String, SystemClock>>,
    requests_total: IntCounter,
    requests_allowed: IntCounter,
    requests_denied: IntCounter,
    clock_errors: IntCounter,
}

impl PrometheusRateLimiter {
    fn new(
        limiter: FluxLimiter<String, SystemClock>,
        registry: &Registry,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let requests_total = IntCounter::new(
            "rate_limiter_requests_total",
            "Total number of rate limit checks"
        )?;
        registry.register(Box::new(requests_total.clone()))?;

        let requests_allowed = IntCounter::new(
            "rate_limiter_requests_allowed",
            "Number of allowed requests"
        )?;
        registry.register(Box::new(requests_allowed.clone()))?;

        let requests_denied = IntCounter::new(
            "rate_limiter_requests_denied",
            "Number of denied requests"
        )?;
        registry.register(Box::new(requests_denied.clone()))?;

        let clock_errors = IntCounter::new(
            "rate_limiter_clock_errors",
            "Number of clock errors"
        )?;
        registry.register(Box::new(clock_errors.clone()))?;

        Ok(Self {
            limiter: Arc::new(limiter),
            requests_total,
            requests_allowed,
            requests_denied,
            clock_errors,
        })
    }

    fn check_request(&self, client_id: String) -> Result<FluxLimiterDecision, FluxLimiterError> {
        self.requests_total.inc();

        let result = self.limiter.check_request(client_id);

        match &result {
            Ok(decision) if decision.allowed => self.requests_allowed.inc(),
            Ok(_) => self.requests_denied.inc(),
            Err(FluxLimiterError::ClockError(_)) => self.clock_errors.inc(),
            _ => {}
        }

        result
    }
}
```

### Logging

Implement structured logging:

```rust
use tracing::{info, warn, error};

fn check_request_with_logging(
    limiter: &FluxLimiter<String, SystemClock>,
    client_id: String,
) -> bool {
    match limiter.check_request(client_id.clone()) {
        Ok(decision) if decision.allowed => {
            info!(
                client_id = %client_id,
                remaining = ?decision.remaining_capacity,
                "Request allowed"
            );
            true
        }
        Ok(decision) => {
            warn!(
                client_id = %client_id,
                retry_after = ?decision.retry_after_seconds,
                "Request rate limited"
            );
            false
        }
        Err(FluxLimiterError::ClockError(e)) => {
            error!(
                client_id = %client_id,
                error = ?e,
                "Clock error in rate limiter - failing open"
            );
            true // Fail-open
        }
        Err(e) => {
            error!(
                client_id = %client_id,
                error = ?e,
                "Unexpected rate limiter error"
            );
            false
        }
    }
}
```

## Memory Management

### Automatic Cleanup

Implement periodic cleanup to prevent unbounded memory growth:

```rust
use tokio::time::{interval, Duration};
use std::sync::Arc;

struct ManagedRateLimiter {
    limiter: Arc<FluxLimiter<String, SystemClock>>,
}

impl ManagedRateLimiter {
    fn new(config: FluxLimiterConfig) -> Result<Self, FluxLimiterError> {
        let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock)?);

        // Start background cleanup task
        let limiter_clone = Arc::clone(&limiter);
        tokio::spawn(async move {
            Self::cleanup_loop(limiter_clone).await;
        });

        Ok(Self { limiter })
    }

    async fn cleanup_loop(limiter: Arc<FluxLimiter<String, SystemClock>>) {
        let mut interval = interval(Duration::from_secs(3600)); // 1 hour

        loop {
            interval.tick().await;

            let threshold = 24 * 60 * 60 * 1_000_000_000u64; // 24 hours

            match limiter.cleanup_stale_clients(threshold) {
                Ok(()) => {
                    // Cleanup succeeded
                }
                Err(e) => {
                    error!("Rate limiter cleanup failed: {}", e);
                }
            }
        }
    }

    fn check_request(&self, client_id: String) -> Result<FluxLimiterDecision, FluxLimiterError> {
        self.limiter.check_request(client_id)
    }
}
```

### Memory Monitoring

Monitor memory usage and alert on growth:

```rust
fn estimate_memory_usage(limiter: &FluxLimiter<String, SystemClock>) -> usize {
    // Estimate: ~48 bytes per client (HashMap overhead + String + u64)
    let estimated_clients = 1_000_000; // Adjust based on your load
    estimated_clients * 48
}

fn check_memory_threshold(limiter: &FluxLimiter<String, SystemClock>) {
    let estimated_usage = estimate_memory_usage(limiter);
    let threshold = 100 * 1024 * 1024; // 100 MB

    if estimated_usage > threshold {
        warn!(
            "Rate limiter memory usage estimated at {} MB",
            estimated_usage / (1024 * 1024)
        );
    }
}
```

## Error Handling Strategy

### Circuit Breaker for Clock Errors

Temporarily disable rate limiting after persistent failures:

```rust
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::time::{SystemTime, Duration};

struct CircuitBreakerRateLimiter {
    limiter: FluxLimiter<String, SystemClock>,
    consecutive_failures: AtomicU64,
    circuit_open: AtomicBool,
    circuit_open_time: AtomicU64,
    failure_threshold: u64,
    reset_timeout_secs: u64,
}

impl CircuitBreakerRateLimiter {
    fn new(
        config: FluxLimiterConfig,
        failure_threshold: u64,
        reset_timeout_secs: u64,
    ) -> Result<Self, FluxLimiterError> {
        Ok(Self {
            limiter: FluxLimiter::with_config(config, SystemClock)?,
            consecutive_failures: AtomicU64::new(0),
            circuit_open: AtomicBool::new(false),
            circuit_open_time: AtomicU64::new(0),
            failure_threshold,
            reset_timeout_secs,
        })
    }

    fn check_request(&self, client_id: String) -> bool {
        // Check if circuit should be reset
        if self.circuit_open.load(Ordering::Relaxed) {
            let open_time = self.circuit_open_time.load(Ordering::Relaxed);
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now - open_time > self.reset_timeout_secs {
                info!("Attempting to close circuit breaker");
                self.circuit_open.store(false, Ordering::Relaxed);
                self.consecutive_failures.store(0, Ordering::Relaxed);
            } else {
                // Circuit still open - bypass rate limiting
                return true;
            }
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
                    error!(
                        "Opening circuit breaker after {} consecutive clock failures",
                        failures
                    );
                    self.circuit_open.store(true, Ordering::Relaxed);
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    self.circuit_open_time.store(now, Ordering::Relaxed);
                }

                true // Fail-open
            }
            Err(e) => {
                error!("Unexpected rate limiter error: {}", e);
                false
            }
        }
    }
}
```

## Load Testing

Test your rate limiter configuration under load:

```rust
#[cfg(test)]
mod load_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    #[test]
    fn test_concurrent_load() {
        let config = FluxLimiterConfig::new(1000.0, 500.0);
        let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

        let start = Instant::now();
        let threads: Vec<_> = (0..10)
            .map(|i| {
                let limiter = Arc::clone(&limiter);
                thread::spawn(move || {
                    let mut allowed = 0;
                    let mut denied = 0;

                    for j in 0..10000 {
                        let client_id = format!("client_{}_{}", i, j % 100);
                        match limiter.check_request(client_id) {
                            Ok(decision) if decision.allowed => allowed += 1,
                            Ok(_) => denied += 1,
                            Err(_) => {}
                        }
                    }

                    (allowed, denied)
                })
            })
            .collect();

        let mut total_allowed = 0;
        let mut total_denied = 0;

        for handle in threads {
            let (allowed, denied) = handle.join().unwrap();
            total_allowed += allowed;
            total_denied += denied;
        }

        let elapsed = start.elapsed();
        let total_requests = total_allowed + total_denied;

        println!("Processed {} requests in {:?}", total_requests, elapsed);
        println!("Throughput: {:.2} req/s", total_requests as f64 / elapsed.as_secs_f64());
        println!("Allowed: {}, Denied: {}", total_allowed, total_denied);
    }
}
```

## Deployment Checklist

- [ ] Configure appropriate rate and burst limits
- [ ] Implement error handling policy (fail-open/fail-closed)
- [ ] Set up monitoring and alerting
- [ ] Enable structured logging
- [ ] Configure automatic cleanup
- [ ] Load test under expected traffic
- [ ] Document rate limit headers in API docs
- [ ] Set up memory monitoring
- [ ] Test clock error handling
- [ ] Implement circuit breaker if needed

## Next Steps

- [Monitoring](./basic-usage.md#using-decision-metadata) - Use decision metadata
- [Performance](../architecture/performance.md) - Understand performance characteristics
- [Error Handling](./error-handling.md) - Handle errors gracefully