# Component Design

Detailed exploration of Flux Limiter's core components and their design rationale.

## FluxLimiter<T, C>

The main rate limiter struct uses generics for flexibility.

### Structure Definition

```rust
#[derive(Debug)]
pub struct FluxLimiter<T, C = SystemClock>
where
    T: Hash + Eq + Clone,  // Client identifier type
    C: Clock,              // Time source
{
    rate_nanos: u64,                    // Rate interval in nanoseconds
    tolerance_nanos: u64,               // Burst tolerance in nanoseconds
    client_state: Arc<DashMap<T, u64>>, // Client TAT storage
    clock: C,                           // Time abstraction
}
```

**Implemented Traits**:
- `Debug` — via `#[derive]`
- `Clone` — manual implementation; requires `C: Clone`. Cloning shares the same `DashMap` via `Arc::clone`, so both the original and clone operate on the same client state.

### Design Rationale

**Generic Client ID (T)**:
- Supports `String`, `IpAddr`, `u64`, custom types
- Constrains: `Hash + Eq + Clone`
- Zero-cost abstraction - no runtime overhead

**Generic Clock (C)**:
- Default: `SystemClock` for production
- Alternative: `TestClock` for testing
- Custom: User-defined time sources
- Constrains: `Clock` trait

**Arc<DashMap>**:
- Thread-safe shared ownership
- Lock-free concurrent access
- Minimal contention
- Efficient cloning

**Nanosecond Storage**:
- `u64` for rate and tolerance
- Maintains precision throughout calculations
- Avoids floating-point arithmetic in hot path

### Public API

```rust
impl<T, C> FluxLimiter<T, C>
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    pub fn with_config(
        config: FluxLimiterConfig,
        clock: C,
    ) -> Result<Self, FluxLimiterError>

    pub fn check_request(
        &self,
        client_id: T,
    ) -> Result<FluxLimiterDecision, FluxLimiterError>

    pub fn cleanup_stale_clients(
        &self,
        max_stale_nanos: u64,
    ) -> Result<(), FluxLimiterError>

    pub fn rate(&self) -> f64
    pub fn burst(&self) -> f64
}
```

## FluxLimiterConfig

Configuration management with builder pattern support.

### Structure Definition

```rust
#[derive(Debug, Clone)]
pub struct FluxLimiterConfig {
    rate_per_second: f64,   // User-friendly rate specification
    burst_capacity: f64,    // User-friendly burst specification
}
```

### Design Rationale

**User-Friendly Units**:
- `rate_per_second`: Intuitive "requests per second"
- `burst_capacity`: Additional burst allowance
- Converted to nanoseconds internally

**Builder Pattern**:
```rust
impl FluxLimiterConfig {
    pub fn new(rate_per_second: f64, burst_capacity: f64) -> Self
    pub fn rate(mut self, rate_per_second: f64) -> Self
    pub fn burst(mut self, burst_capacity: f64) -> Self
    pub fn validate(&self) -> Result<(), FluxLimiterError>
}
```

### Validation

```rust
pub fn validate(&self) -> Result<(), FluxLimiterError> {
    if self.rate_per_second <= 0.0 {
        return Err(FluxLimiterError::InvalidRate);
    }
    if self.burst_capacity < 0.0 {
        return Err(FluxLimiterError::InvalidBurst);
    }
    Ok(())
}
```

**Validation Rules**:
- Rate must be positive (> 0.0)
- Burst must be non-negative (≥ 0.0)
- Checked at construction time

## FluxLimiterDecision

Rich metadata returned from rate limiting decisions.

### Structure Definition

```rust
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FluxLimiterDecision {
    pub allowed: bool,                    // Primary decision
    pub retry_after_seconds: Option<f64>, // When to retry (if denied)
    pub remaining_capacity: Option<f64>,  // Remaining burst capacity
    pub reset_time_nanos: u64,           // When window resets
}
```

The `#[non_exhaustive]` attribute ensures that adding fields in future versions
is not a breaking change. Consumers can still access all public fields via dot
notation, but cannot construct or destructure the struct exhaustively. This is
consistent with `FluxLimiterError`, which is also `#[non_exhaustive]`.

### Design Rationale

**Rich Metadata Enables**:
- HTTP rate limit headers (X-RateLimit-Remaining, Retry-After)
- Client-side backoff strategies
- Monitoring and observability
- Debugging and diagnostics

**Field Details**:

1. **`allowed: bool`**
   - Primary decision
   - `true` = allow request, `false` = deny

2. **`retry_after_seconds: Option<f64>`**
   - `Some(seconds)` when denied
   - How long to wait before retrying
   - Used for HTTP `Retry-After` header

3. **`remaining_capacity: Option<f64>`**
   - Current burst capacity remaining
   - Useful for `X-RateLimit-Remaining` header
   - Helps clients understand their quota

4. **`reset_time_nanos: u64`**
   - Nanosecond timestamp when limit resets
   - Convert to HTTP `X-RateLimit-Reset` header
   - Absolute time, not relative

## FluxLimiterError

Comprehensive error handling for robust production usage.

### Enum Definition

```rust
#[non_exhaustive]
#[derive(Debug)]
pub enum FluxLimiterError {
    InvalidRate,           // Configuration: rate ≤ 0
    InvalidBurst,          // Configuration: burst < 0
    ClockError(ClockError), // Runtime: clock failure
}

#[derive(Debug)]
pub enum ClockError {
    SystemTimeError,       // System time unavailable
}
```

### Design Rationale

**Explicit Error Types**:
- Configuration errors (InvalidRate, InvalidBurst)
- Runtime errors (ClockError)
- Clear separation of error categories

**Error Display**:
```rust
impl std::fmt::Display for FluxLimiterError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::InvalidRate => write!(f, "Rate must be positive"),
            Self::InvalidBurst => write!(f, "Burst must be non-negative"),
            Self::ClockError(err) => write!(f, "Clock error occurred: {}", err),
        }
    }
}
```

## Clock Trait

Abstraction for pluggable time sources.

### Trait Definition

```rust
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<u64, ClockError>;
}
```

### Design Rationale

**Benefits**:
- Enables deterministic testing
- Handles real-world clock issues
- Allows custom time sources
- Graceful error handling

**Send + Sync Requirements**:
- Thread-safe: Can be shared across threads
- Required for concurrent rate limiting

### SystemClock

Production time source using system time:

```rust
#[derive(Debug, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Result<u64, ClockError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .map_err(|_| ClockError::SystemTimeError)
    }
}
```

**Handles Real-World Issues**:
- System clock going backwards (NTP adjustments)
- Clock resolution limitations
- System suspend/resume
- Virtualization time skips

### TestClock

Deterministic time source for testing:

```rust
pub struct TestClock {
    time: Arc<AtomicU64>,        // Current time in nanoseconds
    should_fail: Arc<AtomicBool>, // Failure simulation flag
}

impl TestClock {
    pub fn new(initial_time_secs: f64) -> Self
    pub fn advance(&self, duration_secs: f64)
    pub fn set_time(&self, time_secs: f64)
    pub fn fail_next_call(&self)
}
```

**Features**:
- Precise time control
- Failure simulation
- Thread-safe
- Deterministic testing

## Memory Layout

### FluxLimiter Size

```rust
FluxLimiter<String, SystemClock> {
    rate_nanos: u64,         // 8 bytes
    tolerance_nanos: u64,    // 8 bytes
    client_state: Arc<..>,   // 8 bytes (pointer)
    clock: SystemClock,      // 0 bytes (zero-sized type)
}
// Total: 24 bytes
```

**Cache Efficiency**:
- Small struct size fits in cache line
- Frequently accessed fields grouped
- Arc enables cheap cloning

### Per-Client State

```rust
DashMap<String, u64> entry:
    String: ~24 bytes (pointer + len + capacity)
    u64:    8 bytes
    Overhead: ~16 bytes (hash map metadata)
// Total per client: ~48 bytes
```

### Scalability

```
1,000 clients     = ~48 KB
10,000 clients    = ~480 KB
100,000 clients   = ~4.8 MB
1,000,000 clients = ~48 MB
```

## Thread Safety

All components are thread-safe:

- **FluxLimiter**: Safe to share via `Arc`
- **DashMap**: Lock-free concurrent access
- **Clock**: `Send + Sync` requirement
- **Decisions**: Immutable after creation

## Next Steps

- [Concurrency Model](./concurrency.md) - Understand thread safety
- [Performance Design](./performance.md) - Optimization techniques
- [Error Handling Architecture](./error-handling.md) - Error strategies