# Advanced Usage

Advanced features and optimization techniques for Flux Limiter.

## Memory Management

Flux Limiter tracks state for each client, which can grow over time. Use cleanup to manage memory.

### Understanding Memory Usage

- **Memory**: O(number of active clients)
- **Per-client overhead**: about 31 bytes with a `u64` key and about 76-95 bytes with a short `String` key, measured as the average over a million or more clients
- **It grows in steps, not smoothly**: the client map doubles its tables when it fills, so memory holds flat for a while and then jumps. At any one client count the true figure is between about two thirds and the whole of the average above
- **Example**: 1 million clients take about 34 MB with `u64` keys

### Manual Cleanup

```rust
// Clean up clients that haven't been seen for 1 hour
let one_hour_nanos = 60 * 60 * 1_000_000_000u64;

match limiter.cleanup_stale_clients(one_hour_nanos) {
    Ok(()) => {
        // Cleanup succeeded
    }
    Err(e) => {
        eprintln!("Cleanup failed: {}", e);
        // Cleanup failure is usually not critical - log and continue
    }
}
```

### Automatic Cleanup with Tokio

```rust
use std::sync::Arc;
use tokio::time::{interval, Duration};

async fn start_cleanup_task(limiter: Arc<FluxLimiter<String, MonotonicClock>>) {
    let mut cleanup_interval = interval(Duration::from_secs(3600)); // 1 hour

    loop {
        cleanup_interval.tick().await;

        let threshold = 24 * 60 * 60 * 1_000_000_000u64; // 24 hours

        match limiter.cleanup_stale_clients(threshold) {
            Ok(()) => {
                // Cleanup succeeded
            }
            Err(e) => {
                eprintln!("Cleanup failed: {}", e);
                // Consider implementing fallback cleanup or alerting
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let config = FluxLimiterConfig::new(100.0, 50.0);
    let limiter = Arc::new(FluxLimiter::with_config(config, MonotonicClock::new()).unwrap());

    // Start cleanup task in background
    let limiter_clone = Arc::clone(&limiter);
    tokio::spawn(start_cleanup_task(limiter_clone));

    // Use limiter in your application...
}
```

### Dynamic Cleanup Thresholds

Base cleanup threshold on the configured rate:

```rust
fn calculate_cleanup_threshold(limiter: &FluxLimiter<String, MonotonicClock>) -> u64 {
    // Clean up clients inactive for 100x the rate interval
    let rate = limiter.rate();
    let rate_interval_nanos = (1_000_000_000.0 / rate) as u64;
    rate_interval_nanos * 100
}

let threshold = calculate_cleanup_threshold(&limiter);
let _ = limiter.cleanup_stale_clients(threshold);
```

## Custom Client ID Types

Create custom client IDs for complex rate limiting scenarios.

### Composite Client IDs

Rate limit by multiple dimensions:

```rust
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CompositeClientId {
    user_id: String,
    api_endpoint: String,
}

let limiter = FluxLimiter::<CompositeClientId, _>::with_config(
    FluxLimiterConfig::new(100.0, 50.0),
    MonotonicClock::new()
).unwrap();

// Different rate limits per user per endpoint
let client = CompositeClientId {
    user_id: "user_123".to_string(),
    api_endpoint: "/api/data".to_string(),
};

limiter.check_request(client)?;
```

### Hierarchical Rate Limiting

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum RateLimitKey {
    Global,
    PerTenant(String),
    PerUser(String, String), // tenant_id, user_id
}

let limiter = FluxLimiter::<RateLimitKey, _>::with_config(
    FluxLimiterConfig::new(1000.0, 500.0),
    MonotonicClock::new()
).unwrap();

// Check multiple levels
fn check_hierarchical(
    limiter: &FluxLimiter<RateLimitKey, MonotonicClock>,
    tenant_id: &str,
    user_id: &str,
) -> bool {
    // Global limit
    if !limiter.check_request(RateLimitKey::Global).unwrap().allowed {
        return false;
    }

    // Tenant limit
    if !limiter.check_request(RateLimitKey::PerTenant(tenant_id.to_string())).unwrap().allowed {
        return false;
    }

    // User limit
    limiter.check_request(RateLimitKey::PerUser(tenant_id.to_string(), user_id.to_string()))
        .unwrap()
        .allowed
}
```

## Multiple Rate Limiters

Use different rate limiters for different purposes.

### Tiered Rate Limiting

```rust
use std::sync::Arc;

struct TieredRateLimiter {
    free_tier: Arc<FluxLimiter<String, MonotonicClock>>,
    paid_tier: Arc<FluxLimiter<String, MonotonicClock>>,
    premium_tier: Arc<FluxLimiter<String, MonotonicClock>>,
}

impl TieredRateLimiter {
    fn new() -> Result<Self, FluxLimiterError> {
        Ok(Self {
            free_tier: Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(10.0, 5.0), // 10 req/s
                MonotonicClock::new()
            )?),
            paid_tier: Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(100.0, 50.0), // 100 req/s
                MonotonicClock::new()
            )?),
            premium_tier: Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(1000.0, 500.0), // 1000 req/s
                MonotonicClock::new()
            )?),
        })
    }

    fn check_request(&self, tier: &str, client_id: String) -> Result<FluxLimiterDecision, FluxLimiterError> {
        match tier {
            "free" => self.free_tier.check_request(client_id),
            "paid" => self.paid_tier.check_request(client_id),
            "premium" => self.premium_tier.check_request(client_id),
            _ => self.free_tier.check_request(client_id),
        }
    }
}
```

### Per-Endpoint Rate Limiting

```rust
use std::collections::HashMap;
use std::sync::Arc;

struct EndpointRateLimiter {
    limiters: HashMap<String, Arc<FluxLimiter<String, MonotonicClock>>>,
    default: Arc<FluxLimiter<String, MonotonicClock>>,
}

impl EndpointRateLimiter {
    fn new() -> Result<Self, FluxLimiterError> {
        let mut limiters = HashMap::new();

        // Strict limit for expensive endpoint
        limiters.insert(
            "/api/expensive".to_string(),
            Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(1.0, 2.0),
                MonotonicClock::new()
            )?),
        );

        // Generous limit for cheap endpoint
        limiters.insert(
            "/api/cheap".to_string(),
            Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(1000.0, 500.0),
                MonotonicClock::new()
            )?),
        );

        // Default limit
        let default = Arc::new(FluxLimiter::with_config(
            FluxLimiterConfig::new(100.0, 50.0),
            MonotonicClock::new()
        )?);

        Ok(Self { limiters, default })
    }

    fn check_request(&self, endpoint: &str, client_id: String) -> Result<FluxLimiterDecision, FluxLimiterError> {
        let limiter = self.limiters.get(endpoint).unwrap_or(&self.default);
        limiter.check_request(client_id)
    }
}
```

## Performance Optimization

### Minimize String Allocations

Use references where possible:

```rust
// Instead of allocating new strings
fn check_request_owned(limiter: &FluxLimiter<String, MonotonicClock>, id: String) -> bool {
    limiter.check_request(id).unwrap().allowed
}

// Use string slices and clone only when needed
fn check_request_borrowed(limiter: &FluxLimiter<String, MonotonicClock>, id: &str) -> bool {
    limiter.check_request(id.to_string()).unwrap().allowed
}
```

Or use numeric IDs for better performance:

```rust
// Faster: no string allocation
let limiter = FluxLimiter::<u64, _>::with_config(config, MonotonicClock::new()).unwrap();
limiter.check_request(user_id_numeric).unwrap();
```

### Batch Operations

Process multiple clients efficiently:

```rust
fn check_batch(
    limiter: &FluxLimiter<String, MonotonicClock>,
    client_ids: &[String],
) -> Vec<bool> {
    client_ids
        .iter()
        .map(|id| limiter.check_request(id.clone())
            .map(|d| d.allowed)
            .unwrap_or(false))
        .collect()
}
```

### Parallel Processing

Use rayon for parallel rate limiting checks:

```rust
use rayon::prelude::*;

fn check_parallel(
    limiter: &FluxLimiter<String, MonotonicClock>,
    client_ids: Vec<String>,
) -> Vec<bool> {
    client_ids
        .par_iter()
        .map(|id| limiter.check_request(id.clone())
            .map(|d| d.allowed)
            .unwrap_or(false))
        .collect()
}
```

## Custom Clock Implementation

Implement custom clock sources for special scenarios.

### Mock Clock for Testing

```rust
use flux_limiter::{Clock, ClockError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone)]
struct MockClock {
    time: Arc<AtomicU64>,
}

impl MockClock {
    fn new(initial_time_nanos: u64) -> Self {
        Self {
            time: Arc::new(AtomicU64::new(initial_time_nanos)),
        }
    }

    fn set_time(&self, time_nanos: u64) {
        self.time.store(time_nanos, Ordering::SeqCst);
    }

    fn advance(&self, duration_nanos: u64) {
        self.time.fetch_add(duration_nanos, Ordering::SeqCst);
    }
}

impl Clock for MockClock {
    fn now(&self) -> Result<u64, ClockError> {
        Ok(self.time.load(Ordering::SeqCst))
    }
}

// Use in tests
let clock = MockClock::new(0);
let limiter = FluxLimiter::with_config(config, clock.clone()).unwrap();

// Control time in tests
clock.advance(1_000_000_000); // Advance 1 second
```

### Clock steps

The default `MonotonicClock` is immune to operating-system clock steps. It
reads the OS clock once, when it is first used, and then measures *elapsed*
time. A backwards step (an NTP correction, a manual change, a VM resuming with
a stale clock) therefore has no effect on decisions, and capacity refills
according to how much time really passed.

If you pass `SystemClock` explicitly you get wall-clock time instead, with its
known limitation: a backwards step makes every client look over-limit until the
OS clock catches back up, and no error is reported.

An earlier version of this page showed a workaround to layer on top of
`SystemClock`: clamp its readings so they never go backwards. That is not a
fix. The clamp freezes the limiter's time at the instant of the step, so
nothing refills until the OS clock climbs back past it. Measured, a client that
waits 9.9 real seconds after a 10-second step gets none of the requests it has
earned. Use `MonotonicClock`, which does not have the problem.

## Next Steps

- [Web Integration](./web-integration.md) - Use with web frameworks
- [Production Considerations](./production.md) - Deploy with confidence
- [Performance Design](../architecture/performance.md) - Understand performance characteristics