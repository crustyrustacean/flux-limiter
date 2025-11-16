# Advanced Usage

Advanced features and optimization techniques for Flux Limiter.

## Memory Management

Flux Limiter tracks state for each client, which can grow over time. Use cleanup to manage memory.

### Understanding Memory Usage

- **Memory**: O(number of active clients)
- **Per-client overhead**: ~40-48 bytes (client ID + TAT timestamp)
- **Example**: 1 million clients ≈ 40-48 MB

### Manual Cleanup

```rust
// Clean up clients that haven't been seen for 1 hour
let one_hour_nanos = 60 * 60 * 1_000_000_000u64;

match limiter.cleanup_stale_clients(one_hour_nanos) {
    Ok(removed_count) => {
        println!("Cleaned up {} stale clients", removed_count);
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

async fn start_cleanup_task(limiter: Arc<FluxLimiter<String, SystemClock>>) {
    let mut cleanup_interval = interval(Duration::from_secs(3600)); // 1 hour

    loop {
        cleanup_interval.tick().await;

        let threshold = 24 * 60 * 60 * 1_000_000_000u64; // 24 hours

        match limiter.cleanup_stale_clients(threshold) {
            Ok(count) => {
                println!("Cleaned up {} stale clients", count);
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
    let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock).unwrap());

    // Start cleanup task in background
    let limiter_clone = Arc::clone(&limiter);
    tokio::spawn(start_cleanup_task(limiter_clone));

    // Use limiter in your application...
}
```

### Dynamic Cleanup Thresholds

Base cleanup threshold on the configured rate:

```rust
fn calculate_cleanup_threshold(limiter: &FluxLimiter<String, SystemClock>) -> u64 {
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
    SystemClock
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
    SystemClock
).unwrap();

// Check multiple levels
fn check_hierarchical(
    limiter: &FluxLimiter<RateLimitKey, SystemClock>,
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
    free_tier: Arc<FluxLimiter<String, SystemClock>>,
    paid_tier: Arc<FluxLimiter<String, SystemClock>>,
    premium_tier: Arc<FluxLimiter<String, SystemClock>>,
}

impl TieredRateLimiter {
    fn new() -> Result<Self, FluxLimiterError> {
        Ok(Self {
            free_tier: Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(10.0, 5.0), // 10 req/s
                SystemClock
            )?),
            paid_tier: Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(100.0, 50.0), // 100 req/s
                SystemClock
            )?),
            premium_tier: Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(1000.0, 500.0), // 1000 req/s
                SystemClock
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
    limiters: HashMap<String, Arc<FluxLimiter<String, SystemClock>>>,
    default: Arc<FluxLimiter<String, SystemClock>>,
}

impl EndpointRateLimiter {
    fn new() -> Result<Self, FluxLimiterError> {
        let mut limiters = HashMap::new();

        // Strict limit for expensive endpoint
        limiters.insert(
            "/api/expensive".to_string(),
            Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(1.0, 2.0),
                SystemClock
            )?),
        );

        // Generous limit for cheap endpoint
        limiters.insert(
            "/api/cheap".to_string(),
            Arc::new(FluxLimiter::with_config(
                FluxLimiterConfig::new(1000.0, 500.0),
                SystemClock
            )?),
        );

        // Default limit
        let default = Arc::new(FluxLimiter::with_config(
            FluxLimiterConfig::new(100.0, 50.0),
            SystemClock
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
fn check_request_owned(limiter: &FluxLimiter<String, SystemClock>, id: String) -> bool {
    limiter.check_request(id).unwrap().allowed
}

// Use string slices and clone only when needed
fn check_request_borrowed(limiter: &FluxLimiter<String, SystemClock>, id: &str) -> bool {
    limiter.check_request(id.to_string()).unwrap().allowed
}
```

Or use numeric IDs for better performance:

```rust
// Faster: no string allocation
let limiter = FluxLimiter::<u64, _>::with_config(config, SystemClock).unwrap();
limiter.check_request(user_id_numeric).unwrap();
```

### Batch Operations

Process multiple clients efficiently:

```rust
fn check_batch(
    limiter: &FluxLimiter<String, SystemClock>,
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
    limiter: &FluxLimiter<String, SystemClock>,
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

### Monotonic Clock

Ensure time never goes backward:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct MonotonicClock {
    last_time: Arc<AtomicU64>,
}

impl MonotonicClock {
    fn new() -> Self {
        Self {
            last_time: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Result<u64, ClockError> {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ClockError::SystemTimeError)?
            .as_nanos() as u64;

        // Ensure monotonicity
        let mut last = self.last_time.load(Ordering::Acquire);
        loop {
            let next = current_time.max(last);
            match self.last_time.compare_exchange_weak(
                last,
                next,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(next),
                Err(x) => last = x,
            }
        }
    }
}
```

## Next Steps

- [Web Integration](./web-integration.md) - Use with web frameworks
- [Production Considerations](./production.md) - Deploy with confidence
- [Performance Design](../architecture/performance.md) - Understand performance characteristics
