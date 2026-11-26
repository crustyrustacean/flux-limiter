# Component Design

Detailed exploration of Flux Limiter's core components and their design rationale.

## `FluxLimiter<T, C>`

The main rate limiter struct uses generics for flexibility.

### Structure Definition

```rust
#[derive(Debug)]
pub struct FluxLimiter<T, C = MonotonicClock>
where
    T: Hash + Eq + Clone,  // Client identifier type
    C: Clock,              // Time source
{
    rate_nanos: u64,                    // Rate interval in nanoseconds
    tolerance_nanos: u64,               // Burst tolerance in nanoseconds
    client_state: Arc<DashMap<T, AtomicU64>>, // Client TAT storage
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
- Default: `MonotonicClock`, which is immune to OS clock steps
- Opt-in: `SystemClock` for wall-clock time
- Alternative: `TestClock` for testing
- Custom: User-defined time sources
- Constrains: `Clock` trait

**`Arc<DashMap>`**:
- Thread-safe shared ownership
- Sharded locks: a lookup takes one shard's read lock, a first-time insert its write lock
- Updating a known client bypasses the lock entirely (compare-and-swap on its atomic)
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
    // `NaN` fails every comparison, so `is_finite` must reject it explicitly.
    if !self.rate_per_second.is_finite()
        || self.rate_per_second <= 0.0
        || self.rate_per_second > MAX_RATE_PER_SECOND
    {
        return Err(FluxLimiterError::InvalidRate);
    }
    if !self.burst_capacity.is_finite() || self.burst_capacity < 0.0 {
        return Err(FluxLimiterError::InvalidBurst);
    }
    Ok(())
}
```

**Validation Rules**:
- Rate must be finite, positive (> 0.0), and at most 1,000,000,000 (the interval is whole nanoseconds)
- Burst must be finite and non-negative (≥ 0.0)
- Rate is checked before burst
- Checked at construction time

A separate bound is enforced during construction rather than in `validate()`, because it depends on the derived interval: `burst × (1e9 / rate)` must fit in a `u64`, or construction fails with `InvalidBurst`. Both bounds are compared as `f64` values before any integer cast, because a float-to-integer `as` cast saturates instead of failing.

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
    InvalidRate,           // Configuration: rate non-finite, ≤ 0, or > 1e9
    InvalidBurst,          // Configuration: burst non-finite, < 0, or too large to fit
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
- Lets the limiter use a monotonic clock instead of the OS clock
- Allows custom time sources
- Reports a failed clock reading as an error instead of panicking

**Send + Sync Requirements**:
- Thread-safe: Can be shared across threads
- Required for concurrent rate limiting

### MonotonicClock

The default clock. It records an anchor (an `Instant` paired with the OS clock's
reading at that moment) the first time it is used, then reports
`anchor + elapsed`:

```rust
#[derive(Debug, Clone, Default)]
pub struct MonotonicClock {
    anchor: Arc<OnceLock<Anchor>>,
    // ...
}

impl Clock for MonotonicClock {
    fn now(&self) -> Result<u64, ClockError> {
        // anchor is set on the first call; later calls only read elapsed time
        // ...
    }
}
```

**Properties**:
- Monotonic: readings never go backwards, whatever the OS clock does
- Advances by real elapsed time, so capacity refills correctly while the OS clock is wrong
- Still a Unix-epoch timestamp in nanoseconds, within microseconds of the OS clock for as long as that clock is not stepped
- Never returns `ClockError`; `Instant::elapsed` cannot fail
- 40 bytes; clones share one anchor, so a cloned limiter and its original see the same time

### SystemClock

The opt-in wall-clock time source, for callers who need readings that match the OS clock exactly. Pass it explicitly: `FluxLimiter::with_config(config, SystemClock)`.

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

**Known limitations**:
- A clock that steps backwards is **not** a source of `ClockError`: `SystemTime::now()` still succeeds, it simply returns an earlier value. Such a step makes clients look over-limit until the clock catches back up; this is why it is not the default; use `MonotonicClock` unless you need wall-clock time.
- Clock resolution is whatever the OS provides; the nanosecond value is only as fine as that
- Suspend/resume and VM migration can produce a discontinuity, with the same consequence as above

### TestClock

Deterministic time source for testing. It is part of the test suite
(`tests/ratelimiter/fixtures/test_clock.rs`), not of the crate, because the
`Clock` trait lets you write your own:

```rust
#[derive(Debug, Clone)]
pub struct TestClock {
    time: Arc<AtomicU64>,         // Current time in nanoseconds
    should_fail: Arc<AtomicBool>, // Failure simulation flag
}

impl TestClock {
    pub fn new(initial_time_secs: f64) -> Self
    pub fn advance(&self, duration_secs: f64)
    pub fn set_time(&self, time_secs: f64)
    pub fn set_nanos(&self, nanos: u64)      // exact; set_time takes f64
    pub fn fail_next_call(&self)             // the next now() returns Err
    pub fn reset_failure(&self)
    pub fn time_as_f64(&self) -> f64
}
```

**Features**:
- Precise time control. A test sets the clock to an exact instant instead of
  waiting, so it runs instantly and always gives the same answer
- `set_nanos` sets an exact nanosecond value. `set_time` takes an `f64` number
  of seconds, which cannot represent a current epoch timestamp exactly
- Failure simulation, for testing the error path
- Clones share one time, so the limiter and the test move the same clock
- Thread-safe
- Deterministic testing

## Memory Layout

### FluxLimiter Size

```rust
FluxLimiter<String, MonotonicClock> {   // 64 bytes in total
    rate_nanos: u64,         // 8 bytes
    tolerance_nanos: u64,    // 8 bytes
    client_state: Arc<..>,   // 8 bytes (pointer)
    clock: MonotonicClock,   // 40 bytes
}
```

**Size**: 64 bytes with the default clock, which is one 64-byte cache line, and 24
bytes with `SystemClock`. The size does not depend on the client ID type.

**Cache Efficiency**:
- The limiter with the default clock occupies exactly one cache line
- Frequently accessed fields grouped
- Arc enables cheap cloning

### Per-Client State

One map entry per client, holding the key and its TAT. Memory is allocated
in steps: DashMap grows its tables by doubling, so the total is flat for a
while and then jumps. Measured on a 64-bit target (Windows system allocator):

| Key                   | Average bytes per client, 1M+ clients | Range across client counts |
|-----------------------|---------------------------------------|-----------------------------|
| `u64`                 | ~31                                   | 24 to 36                    |
| `String`, 15 chars    | ~76                                   | 63 to 86                    |
| `String`, 36 chars    | ~95                                   | 82 to 105                   |

A `String` key costs the map entry plus the key's characters, and the
allocator rounds those up to a power of two (16, 32, 64 bytes).
One million clients take about 34 MB with `u64` keys.

### Scalability

Measured with `u64` keys on a 64-bit target:

| Clients | Memory |
|---|---|
| 1,000 | 46 KB |
| 10,000 | 0.3 MB |
| 100,000 | 2.1 MB |
| 1,000,000 | 34 MB |
| 10,000,000 | 272 MB |

Short `String` keys take about two and a half to three times as much.

## Thread Safety

All components are thread-safe:

- **FluxLimiter**: Safe to share via `Arc`
- **DashMap**: Sharded concurrent access; per-client updates are lock-free
- **Clock**: `Send + Sync` requirement
- **Decisions**: Immutable after creation

## Next Steps

- [Concurrency Model](./concurrency.md) - Understand thread safety
- [Performance Design](./performance.md) - Optimization techniques
- [Error Handling Architecture](./error-handling.md) - Error strategies