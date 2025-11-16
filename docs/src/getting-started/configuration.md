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

- **Must be positive** (> 0)
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

- **Must be non-negative** (≥ 0)
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
match FluxLimiter::with_config(config, SystemClock) {
    Ok(_) => println!("Valid configuration"),
    Err(FluxLimiterError::InvalidRate) => {
        eprintln!("Error: Rate must be positive");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Validation Rules

- **InvalidRate**: Returned if rate ≤ 0
- **InvalidBurst**: Returned if burst < 0

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
    limiter: Arc<RwLock<FluxLimiter<String, SystemClock>>>,
}

impl DynamicRateLimiter {
    fn update_config(&self, new_config: FluxLimiterConfig) {
        let new_limiter = FluxLimiter::with_config(new_config, SystemClock).unwrap();
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
