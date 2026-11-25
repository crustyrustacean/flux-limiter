# Configuration

Learn how to configure Flux Limiter for your specific use case.

## Configuration Basics

Flux Limiter uses `FluxLimiterConfig` for configuration:

```rust
use flux_limiter::FluxLimiterConfig;

let config = FluxLimiterConfig::new(rate_per_second, burst_capacity);
```

## Rate Parameter

The **rate** parameter defines the sustained requests per second.

- **Must be finite**: `NaN` and `±Infinity` are rejected
- **Must be positive** (> 0)
- **Must be at most `1_000_000_000`** (1 billion requests per second): the rate interval is stored as whole nanoseconds, so a higher rate has no representable interval
- Defines the steady-state request rate
- Example: `rate = 100.0` means 100 requests per second

```rust
// 100 requests per second sustained
let config = FluxLimiterConfig::new(100.0, 0.0);
```

### Understanding Rate

The rate translates to a time interval between requests:

- `rate = 1.0` → One request per second (1000ms interval)
- `rate = 10.0` → Ten requests per second (100ms interval)
- `rate = 100.0` → One hundred requests per second (10ms interval)

## Burst Parameter

The **burst** parameter defines additional capacity for handling bursts.

- **Must be finite**: `NaN` and `±Infinity` are rejected
- **Must be non-negative** (≥ 0)
- **Must be small enough to fit**: `burst × (1_000_000_000 / rate)` is stored as a 64-bit nanosecond count, so a burst whose tolerance would exceed `2^64` nanoseconds (about 584 years) is rejected
- Allows temporary spikes above the sustained rate
- Example: `burst = 50.0` allows bursts of ~50 additional requests

```rust
// 100 requests per second with burst capacity of 50
let config = FluxLimiterConfig::new(100.0, 50.0);
```

### Understanding Burst

- **Total capacity**: Approximately `1 + burst` requests can be made immediately
- **Burst recovery**: Unused burst capacity accumulates at the configured rate
- **Maximum burst**: Capped at the configured burst value

### Rate and Burst Example

With `rate=10.0` and `burst=5.0`:

1. **Initial state**: Client can make ~6 requests immediately (1 + 5 burst)
2. **After burst**: Limited to sustained rate of 10 requests per second
3. **Recovery**: Burst capacity recovers at 10 units per second
4. **After 1 second idle**: Can make ~6 requests again

## Builder Pattern

Use the builder pattern for clearer configuration:

```rust
let config = FluxLimiterConfig::new(0.0, 0.0)
    .rate(100.0)        // 100 requests per second
    .burst(50.0);       // Allow bursts of up to 50 requests
```

This approach is especially useful when you want to:
- Start with default values and override specific settings
- Make configuration changes clearer and more readable

## Common Configuration Patterns

### Strict Rate Limiting (No Burst)

```rust
// Exactly 10 requests per second, no burst allowed
let config = FluxLimiterConfig::new(10.0, 0.0);
```

Use when:
- You need strict, predictable rate limiting
- Bursts would cause issues for your backend
- You want simple, consistent behavior

### Flexible Rate Limiting (With Burst)

```rust
// 10 requests per second with burst of 20
let config = FluxLimiterConfig::new(10.0, 20.0);
```

Use when:
- User experience benefits from handling short bursts
- Your backend can handle temporary spikes
- You want to allow occasional bursty traffic

### API Gateway Configuration

```rust
// 1000 requests per second with generous burst
let config = FluxLimiterConfig::new(1000.0, 500.0);
```

Use for:
- High-throughput API gateways
- Services that can handle significant load
- Preventing extreme abuse while allowing normal traffic

### Free Tier API Configuration

```rust
// 10 requests per minute (0.1667 per second) with small burst
let rate_per_minute = 10.0 / 60.0;
let config = FluxLimiterConfig::new(rate_per_minute, 5.0);
```

Use for:
- Free tier API limits
- Generous limits that prevent abuse
- Pay-per-use API tiers

## Configuration Validation

Configuration is validated when creating the rate limiter:

```rust
use flux_limiter::{FluxLimiterConfig, FluxLimiterError};

// Invalid configuration: negative rate
let config = FluxLimiterConfig::new(-10.0, 5.0);
match FluxLimiter::with_config(config, MonotonicClock::new()) {
    Ok(_) => println!("Valid configuration"),
    Err(FluxLimiterError::InvalidRate) => {
        eprintln!("Error: Rate must be positive");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Validation Rules

- **InvalidRate**: Returned if rate is not finite, is ≤ 0, or is > 1,000,000,000
- **InvalidBurst**: Returned if burst is not finite, is < 0, or is too large for the configured rate

Rate is checked first: a configuration with both an invalid rate and an invalid burst reports `InvalidRate`.

Every rejected configuration is rejected at construction time as well, so `FluxLimiter::with_config` never returns a limiter built from invalid values.

### Precision: whole-nanosecond truncation

The rate interval (`1_000_000_000 / rate`) and the burst tolerance (`burst × interval`) are stored as whole nanoseconds. Values that are not a whole number of nanoseconds are **truncated**, not rounded. This is deliberate: integer arithmetic keeps every timing decision exact and reproducible.

In practice this is invisible at ordinary rates, but it has two visible consequences:

- **A fractional burst can truncate to nothing, but only at very high rates.** The tolerance is `burst × interval` in nanoseconds, so it is lost entirely when that product is under 1 ns. At `rate = 10.0` the interval is 100 ms and even `burst = 0.004` keeps a 0.4 ms tolerance. But at `rate = 1e8` the interval is 10 ns, so `burst = 0.05` asks for 0.5 ns and gets no burst at all, while `burst = 0.1` (exactly 1 ns) is preserved.
- **A rate that does not divide one billion is enforced higher than requested.** The interval is exact only when `1_000_000_000 / rate` is a whole number. Below about 10 million per second the discarded fraction is under 1% and not noticeable. Above that it grows quickly, because the interval is only a few nanoseconds and a fraction of one is a large share of it. For example `rate = 6e8` has a true interval of 1.667 ns, which truncates to 1 ns, so the limiter enforces 1e9 requests per second: 67% more than asked for.

| Requested rate | Interval (ns) | Enforced rate | Difference |
|---|---|---|---|
| 1e8 | 10 | 1.0e8 | 0% |
| 1.5e8 | 6 | 1.6667e8 | +11.1% |
| 2.5e8 | 4 | 2.5e8 | 0% |
| 4e8 | 2 | 5.0e8 | +25.0% |
| 6e8 | 1 | 1.0e9 | +66.7% |

If you need an exact limit, choose a rate that divides 1,000,000,000 evenly (such as `1e8`, `1e6`, `1000.0`, `10.0`, `1.0`); those are enforced exactly.

### Manual Validation

You can validate configuration before creating the limiter:

```rust
match config.validate() {
    Ok(_) => println!("Configuration is valid"),
    Err(FluxLimiterError::InvalidRate) => {
        eprintln!("Rate must be positive");
    }
    Err(FluxLimiterError::InvalidBurst) => {
        eprintln!("Burst must be non-negative");
    }
    Err(e) => eprintln!("Configuration error: {}", e),
}
```

## Dynamic Configuration

While Flux Limiter doesn't support runtime configuration changes, you can work around this:

```rust
use std::sync::Arc;
use parking_lot::RwLock;

struct DynamicRateLimiter {
    limiter: Arc<RwLock<FluxLimiter<String, MonotonicClock>>>,
}

impl DynamicRateLimiter {
    fn update_config(&self, new_config: FluxLimiterConfig) {
        let new_limiter = FluxLimiter::with_config(new_config, MonotonicClock::new()).unwrap();
        *self.limiter.write() = new_limiter;
    }

    fn check_request(&self, client_id: String) -> Result<FluxLimiterDecision, FluxLimiterError> {
        self.limiter.read().check_request(client_id)
    }
}
```

**Note**: Changing configuration resets all client state.

## Next Steps

- [Basic Usage](../guide/basic-usage.md) - Learn common usage patterns
- [Advanced Usage](../guide/advanced-usage.md) - Explore advanced features
- [Web Integration](../guide/web-integration.md) - Integrate with web frameworks
