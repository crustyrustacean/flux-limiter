# GCRA Algorithm

Deep dive into the Generic Cell Rate Algorithm implementation in Flux Limiter.

## Algorithm Choice

**Generic Cell Rate Algorithm (GCRA)** was chosen over Token Bucket for several reasons:

1. **Mathematical Precision**: Avoids floating-point precision issues
2. **Stateless Calculation**: No background token refill processes
3. **Efficient State**: One timestamp per client vs. token count + last refill
4. **Deterministic**: Exact timing calculations with integer arithmetic

## GCRA vs Token Bucket

### Token Bucket

- Maintains a count of available tokens
- Tokens refill at a constant rate
- Requests consume tokens
- Requires background refill process or calculation on each check

### GCRA

- Maintains Theoretical Arrival Time (TAT)
- Checks if current time conforms to rate
- Updates TAT for next request
- No background processes needed

**Equivalence**: GCRA and Token Bucket produce mathematically equivalent results, but GCRA is more efficient for implementation.

## Core Concepts

### Theoretical Arrival Time (TAT)

The TAT represents the theoretical time when the next request should arrive to maintain the configured rate.

```
Initial state:    TAT = 0
After request 1:  TAT = current_time + rate_interval
After request 2:  TAT = max(current_time, previous_TAT) + rate_interval
```

### Rate Interval (T)

Time between consecutive requests at the configured rate:

```
T = 1 / rate_per_second
T_nanos = 1_000_000_000 / rate_per_second
```

Examples:
- `rate = 10.0` → `T = 0.1s` = `100_000_000 nanos`
- `rate = 100.0` → `T = 0.01s` = `10_000_000 nanos`
- `rate = 1000.0` → `T = 0.001s` = `1_000_000 nanos`

### Tolerance (τ)

Maximum allowed deviation from the rate, determined by burst capacity:

```
τ = burst_capacity * rate_interval
τ_nanos = burst_capacity * (1_000_000_000 / rate_per_second)
```

Examples with `rate = 10.0`:
- `burst = 0.0` → `τ = 0 nanos` (no burst)
- `burst = 5.0` → `τ = 500_000_000 nanos` (0.5 seconds)
- `burst = 10.0` → `τ = 1_000_000_000 nanos` (1 second)

## Algorithm Implementation

### Conformance Check

```rust
let current_time_nanos = clock.now()?;
let previous_tat_nanos = client_state
    .get(&client_id)
    .unwrap_or(current_time_nanos);

// Check if request conforms (is within tolerance)
let is_conforming = current_time_nanos >=
    previous_tat_nanos.saturating_sub(tolerance_nanos);
```

**Conforming Request**: `current_time >= TAT - τ`

This allows requests to arrive up to `τ` nanoseconds early (burst capacity).

### TAT Update (Allowed Request)

```rust
if is_conforming {
    // Allow request and update TAT
    let new_tat_nanos = current_time_nanos
        .max(previous_tat_nanos) + rate_nanos;

    client_state.insert(client_id, new_tat_nanos);

    // Return allowed decision
}
```

**TAT Update Rule**: `TAT' = max(current_time, previous_TAT) + T`

This ensures TAT advances by the rate interval while preventing it from going backward.

### Retry After Calculation (Denied Request)

```rust
else {
    // Deny request
    let retry_after_nanos = previous_tat_nanos
        .saturating_sub(tolerance_nanos)
        .saturating_sub(current_time_nanos);

    let retry_after_seconds = retry_after_nanos as f64 / 1_000_000_000.0;

    // Return denied decision with retry_after
}
```

**Retry After**: Time until the request would conform to the rate.

## Mathematical Foundation

### Basic Equations

1. **Rate Interval**: `T = 1 / r` where `r` is rate per second
2. **Tolerance**: `τ = b * T` where `b` is burst capacity
3. **TAT Update**: `TAT' = max(t, TAT) + T` where `t` is current time
4. **Conformance**: Request allowed if `t >= TAT - τ`

### Burst Capacity

The burst capacity determines how many requests can be made immediately:

```
Total immediate capacity ≈ 1 + burst_capacity
```

**Proof**:
- First request at `t=0`: TAT becomes `T`
- Can make requests at times: `0, T-τ, 2T-τ, 3T-τ, ...`
- Maximum burst when all accumulated: `τ/T = burst_capacity`
- Plus the current request: `1 + burst_capacity`

### Burst Recovery

After using burst capacity, it recovers at the configured rate:

```
Recovery rate = rate_per_second
Time to full recovery = burst_capacity / rate_per_second
```

Example with `rate=10.0`, `burst=5.0`:
- Can burst 6 requests immediately
- Recovers at 10 units/second
- Full recovery in 0.5 seconds

## Precision Guarantees

### Nanosecond Arithmetic

All calculations use `u64` nanoseconds:

```rust
// No floating-point drift
let rate_nanos: u64 = (1_000_000_000.0 / rate_per_second) as u64;
let tolerance_nanos: u64 = (burst_capacity * rate_nanos as f64) as u64;
```

**Benefits**:
- Exact integer arithmetic
- No accumulating rounding errors
- Supports rates up to 1 billion requests/second
- Nanosecond precision for timing

### Overflow Protection

Uses saturating arithmetic to prevent overflow:

```rust
let is_conforming = current_time_nanos >=
    previous_tat_nanos.saturating_sub(tolerance_nanos);
```

**Saturating operations**:
- `saturating_sub`: Returns 0 if underflow would occur
- `saturating_add`: Returns `u64::MAX` if overflow would occur
- Prevents undefined behavior and panics

## Example Scenarios

### Scenario 1: Sustained Rate

Configuration: `rate=10.0`, `burst=0.0`

```
t=0.0s:   Request → Allowed (TAT=0.1s)
t=0.1s:   Request → Allowed (TAT=0.2s)
t=0.2s:   Request → Allowed (TAT=0.3s)
t=0.25s:  Request → Denied (retry after 0.05s)
t=0.3s:   Request → Allowed (TAT=0.4s)
```

### Scenario 2: Burst Capacity

Configuration: `rate=10.0`, `burst=5.0`

```
t=0.0s:   Request → Allowed (TAT=0.1s)
t=0.0s:   Request → Allowed (TAT=0.2s)  [burst]
t=0.0s:   Request → Allowed (TAT=0.3s)  [burst]
t=0.0s:   Request → Allowed (TAT=0.4s)  [burst]
t=0.0s:   Request → Allowed (TAT=0.5s)  [burst]
t=0.0s:   Request → Allowed (TAT=0.6s)  [burst]
t=0.0s:   Request → Denied (retry after 0.1s)
t=0.1s:   Request → Allowed (TAT=0.7s)
```

### Scenario 3: Recovery After Idle

Configuration: `rate=10.0`, `burst=5.0`

```
t=0.0s:   6 requests → All allowed, TAT=0.6s
t=1.0s:   Request → Allowed (TAT=1.1s)  [burst recovered]
t=1.0s:   6 requests → All allowed, TAT=1.6s
```

## Metadata Calculation

### Remaining Capacity

```rust
let elapsed = current_time_nanos.saturating_sub(previous_tat_nanos);
let recovered = (elapsed as f64 / rate_nanos as f64).min(burst_capacity);
let remaining_capacity = Some(burst_capacity - consumed + recovered);
```

Tracks how much burst capacity is currently available.

### Reset Time

```rust
let reset_time_nanos = new_tat_nanos + tolerance_nanos;
```

When the rate limit window will fully reset (all burst capacity recovered).

## Performance Characteristics

- **Time Complexity**: O(1) - constant time for all operations
- **Space Complexity**: O(1) per client - single u64 timestamp
- **Calculation Overhead**: ~10-20 CPU cycles for GCRA calculation
- **Memory Access**: Single DashMap lookup/insert

## Next Steps

- [Component Design](./components.md) - Explore the FluxLimiter struct
- [Performance Design](./performance.md) - Understand optimization techniques
- [Testing Architecture](./testing.md) - Learn about deterministic testing
