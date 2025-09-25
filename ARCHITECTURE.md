# Flux Limiter Architecture

This document provides a comprehensive overview of the Flux Limiter's architecture, design decisions, and implementation details.

## Table of Contents

- [Overview](#overview)
- [Core Architecture](#core-architecture)
- [GCRA Algorithm](#gcra-algorithm)
- [Component Design](#component-design)
- [Concurrency Model](#concurrency-model)
- [Error Handling Architecture](#error-handling-architecture)
- [Performance Design](#performance-design)
- [Testing Architecture](#testing-architecture)
- [Design Decisions](#design-decisions)
- [Future Extensibility](#future-extensibility)

## Overview

Flux Limiter is a high-performance, thread-safe rate limiter implementing the Generic Cell Rate Algorithm (GCRA). The architecture prioritizes:

- **Performance**: O(1) operations with nanosecond precision
- **Correctness**: Mathematically precise algorithm implementation
- **Reliability**: Comprehensive error handling and graceful degradation
- **Flexibility**: Generic client IDs and configurable policies
- **Observability**: Rich metadata for monitoring and HTTP headers

### Key Architectural Principles

1. **Lock-Free Concurrency**: Uses atomic operations and lock-free data structures
2. **Zero-Allocation Hot Path**: Minimizes memory allocation in rate limiting decisions
3. **Clock Abstraction**: Enables testing and handles time-related failures
4. **Type Safety**: Leverages Rust's type system for correctness guarantees
5. **Graceful Degradation**: Continues operation despite partial failures

## Core Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Flux Limiter                             │
├─────────────────────────────────────────────────────────────┤
│  Client API                                                 │
│  ├─ check_request(client_id) -> Result<Decision, Error>     │
│  ├─ cleanup_stale_clients(threshold) -> Result<(), Error>   │
│  └─ rate(), burst() -> f64                                  │
├─────────────────────────────────────────────────────────────┤
│  Core Components                                            │
│  ├─ FluxLimiter<T, C>     │ Main rate limiter struct        │
│  ├─ FluxLimiterConfig     │ Configuration management        │
│  ├─ FluxLimiterDecision   │ Rich decision metadata          │
│  └─ FluxLimiterError      │ Comprehensive error handling    │
├─────────────────────────────────────────────────────────────┤
│  Algorithm Layer                                            │
│  ├─ GCRA Implementation   │ Generic Cell Rate Algorithm     │
│  ├─ Nanosecond Precision  │ u64 nanosecond calculations     │
│  └─ TAT Tracking          │ Theoretical Arrival Time        │
├─────────────────────────────────────────────────────────────┤
│  Storage Layer                                              │
│  ├─ DashMap<T, u64>       │ Lock-free concurrent hash map   │
│  ├─ Atomic Operations     │ Thread-safe state updates       │
│  └─ Memory Management     │ Automatic cleanup mechanisms    │
├─────────────────────────────────────────────────────────────┤
│  Time Abstraction                                           │
│  ├─ Clock Trait           │ Pluggable time source           │
│  ├─ SystemClock           │ Production time implementation   │
│  └─ TestClock             │ Deterministic test time         │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

```
Request → check_request(client_id)
    ↓
Clock::now() → Current Time (nanoseconds)
    ↓
DashMap::get(client_id) → Previous TAT
    ↓
GCRA Calculation
    ↓
Decision: Allow/Deny + Metadata
    ↓
DashMap::insert(client_id, new_TAT)
    ↓
Return FluxLimiterDecision
```

## GCRA Algorithm

### Algorithm Choice

**Generic Cell Rate Algorithm (GCRA)** was chosen over Token Bucket for several reasons:

1. **Mathematical Precision**: Avoids floating-point precision issues
2. **Stateless Calculation**: No background token refill processes
3. **Efficient State**: One timestamp per client vs. token count + last refill
4. **Deterministic**: Exact timing calculations with integer arithmetic

### GCRA Implementation Details

The algorithm maintains a **Theoretical Arrival Time (TAT)** for each client:

```rust
// Core GCRA logic
let current_time_nanos = clock.now()?;
let previous_tat_nanos = client_state.get(&client_id).unwrap_or(current_time_nanos);

// Check if request conforms (is within tolerance)
let is_conforming = current_time_nanos >= previous_tat_nanos.saturating_sub(tolerance_nanos);

if is_conforming {
    // Allow request and update TAT
    let new_tat_nanos = current_time_nanos.max(previous_tat_nanos) + rate_nanos;
    client_state.insert(client_id, new_tat_nanos);
    // Return allowed decision
} else {
    // Deny request
    let retry_after_nanos = previous_tat_nanos.saturating_sub(tolerance_nanos).saturating_sub(current_time_nanos);
    // Return denied decision with retry_after
}
```

### Mathematical Foundation

- **Rate Interval (T)**: `1 / rate_per_second` seconds = `1_000_000_000 / rate_per_second` nanoseconds
- **Tolerance (τ)**: `burst_capacity * rate_interval` nanoseconds
- **TAT Update**: `TAT' = max(current_time, previous_TAT) + T`
- **Conformance Test**: `current_time >= TAT - τ`

### Precision Guarantees

- All calculations use `u64` nanoseconds (no floating-point drift)
- Supports rates up to 1 billion requests/second with nanosecond precision
- Handles edge cases with saturating arithmetic to prevent overflow

## Component Design

### FluxLimiter<T, C>

The core struct uses generics for flexibility:

```rust
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

**Design Rationale**:
- **Generic Client ID**: Supports `String`, `IpAddr`, `u64`, custom types
- **Clock Abstraction**: Enables testing and handles time failures
- **Arc<DashMap>**: Thread-safe, lock-free concurrent access
- **Nanosecond Storage**: Maintains precision throughout calculations

### Configuration System

```rust
#[derive(Debug, Clone)]
pub struct FluxLimiterConfig {
    rate_per_second: f64,   // User-friendly rate specification
    burst_capacity: f64,    // User-friendly burst specification
}
```

**Builder Pattern Support**:
```rust
let config = FluxLimiterConfig::new(0.0, 0.0)
    .rate(100.0)
    .burst(50.0);
```

**Validation**:
- Rate must be positive (> 0.0)
- Burst must be non-negative (≥ 0.0)
- Validation occurs at construction time

### Decision Metadata

```rust
pub struct FluxLimiterDecision {
    pub allowed: bool,                    // Primary decision
    pub retry_after_seconds: Option<f64>, // When to retry (if denied)
    pub remaining_capacity: Option<f64>,  // Remaining burst capacity
    pub reset_time_nanos: u64,           // When window resets
}
```

**Rich Metadata Enables**:
- HTTP rate limit headers (X-RateLimit-Remaining, Retry-After)
- Client-side backoff strategies
- Monitoring and observability
- Debugging and diagnostics

## Concurrency Model

### Lock-Free Design

```
Multiple Threads
      ↓
FluxLimiter (Shared)
      ↓
Arc<DashMap<ClientId, TAT>>
      ↓
Lock-free hash map operations
```

**Thread Safety Guarantees**:
1. **Read Operations**: Multiple concurrent readers without contention
2. **Write Operations**: Atomic updates with minimal contention
3. **Memory Ordering**: Relaxed ordering sufficient for TAT updates
4. **ABA Prevention**: TAT values are monotonically increasing

### DashMap Choice

**Why DashMap over alternatives**:
- **Better than std::HashMap + Mutex**: Avoids global locking
- **Better than RwLock**: No reader/writer contention
- **Better than atomic maps**: Mature, battle-tested implementation
- **Segmented locking**: Reduces contention compared to single lock

### Memory Consistency

```rust
// TAT updates are atomic and isolated per client
client_state.insert(client_id, new_tat);  // Atomic operation

// Concurrent access to different clients is lock-free
// Concurrent access to same client is serialized by DashMap
```

## Error Handling Architecture

### Error Type Hierarchy

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

### Error Propagation Flow

```
Clock::now() → Result<u64, ClockError>
     ↓
FluxLimiter::check_request() → Result<Decision, FluxLimiterError>
     ↓
Application Layer → Implements error policy
```

### Recovery Strategies

1. **Configuration Errors**: Fail fast at startup
2. **Clock Errors**: Graceful degradation options:
   - **Fail-open**: Allow requests during clock failures
   - **Fail-closed**: Deny requests during clock failures
   - **Fallback**: Use alternative rate limiting

### Clock Abstraction Benefits

```rust
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<u64, ClockError>;
}
```

**Handles Real-World Issues**:
- System clock going backwards (NTP adjustments)
- Clock resolution limitations
- System suspend/resume
- Virtualization time skips

## Performance Design

### Hot Path Optimization

The `check_request()` method is optimized for minimal latency:

1. **Single Clock Call**: One time source access per request
2. **Single Map Operation**: Either get or get+insert
3. **Integer Arithmetic**: No floating-point operations
4. **No Allocations**: Reuses existing memory
5. **Minimal Branching**: Straight-line execution

### Memory Layout

```rust
FluxLimiter {
    rate_nanos: u64,         // 8 bytes - cache-friendly
    tolerance_nanos: u64,    // 8 bytes
    client_state: Arc<..>,   // 8 bytes - pointer to shared state
    clock: C,                // Usually zero-sized for SystemClock
}
```

**Cache Efficiency**:
- Small struct size (24-32 bytes)
- Frequently accessed fields grouped together
- Arc enables sharing without duplication

### Algorithmic Complexity

- **Time**: O(1) for `check_request()`
- **Space**: O(number of active clients)
- **Cleanup**: O(number of clients) but infrequent
- **Contention**: O(1) per client, lock-free across clients

### Scalability Characteristics

```
Clients     Memory Usage    Latency
1K          ~32KB          < 1μs
100K        ~3.2MB         < 1μs  
1M          ~32MB          < 1μs
10M         ~320MB         < 1μs
```

## Testing Architecture

### Test Clock Design

```rust
pub struct TestClock {
    time: Arc<AtomicU64>,        // Current time in nanoseconds
    should_fail: Arc<AtomicBool>, // Failure simulation flag
}
```

**Key Features**:
- **Deterministic**: Controlled time progression
- **Thread-safe**: Can be shared across test threads
- **Failure Simulation**: Can simulate clock errors
- **Precise Control**: Nanosecond-level time manipulation

### Test Organization

```
tests/ratelimiter/
├── fixtures/
│   ├── test_clock.rs      # TestClock implementation
│   └── mod.rs
├── gcra_algorithm_tests.rs # Core algorithm correctness
├── config_tests.rs        # Configuration validation
├── error_tests.rs         # Error handling and recovery
├── cleanup_tests.rs       # Memory management
├── performance_tests.rs   # Performance characteristics
└── main.rs               # Test module organization
```

### Error Testing Strategy

```rust
// Simulate clock failure
clock.fail_next_call();
let result = limiter.check_request("client1");
assert!(result.is_err());

// Verify recovery
let result = limiter.check_request("client1");  
assert!(result.is_ok());
```

**Comprehensive Error Coverage**:
- Clock failures during rate limiting
- Clock failures during cleanup
- Multiple consecutive failures
- Recovery after failures
- Partial operation failures

## Design Decisions

### Algorithm Selection: GCRA vs Token Bucket

**GCRA Advantages**:
- ✅ Exact mathematical precision
- ✅ No background processes
- ✅ Simpler state management
- ✅ Better burst handling

**Token Bucket Drawbacks**:
- ❌ Floating-point precision drift
- ❌ Requires background token refill
- ❌ More complex state (tokens + timestamp)
- ❌ Timer management overhead

### Data Structure Selection: DashMap vs Alternatives

**DashMap Benefits**:
- ✅ Lock-free reads and writes
- ✅ Mature and well-tested
- ✅ Good performance characteristics
- ✅ Segmented locking reduces contention

**Alternative Rejections**:
- **HashMap + Mutex**: Global locking bottleneck
- **RwLock<HashMap>**: Reader/writer contention
- **Custom lock-free map**: Development complexity

### Time Representation: Nanoseconds vs Duration

**Nanosecond Choice**:
- ✅ Integer arithmetic (no floating-point errors)
- ✅ Maximum precision for high-rate scenarios
- ✅ Simple overflow handling with saturating operations
- ✅ Direct system time compatibility

### Error Handling: Result Types vs Panics

**Result Type Benefits**:
- ✅ Explicit error handling
- ✅ Graceful degradation options
- ✅ Better observability
- ✅ Production-ready reliability

**Panic Rejection**:
- ❌ Difficult to recover from
- ❌ Poor user experience
- ❌ Hard to monitor and debug

### Generic Design: Client ID Types

**Generic Benefits**:
- ✅ Flexibility for different use cases
- ✅ Zero-cost abstractions
- ✅ Type safety
- ✅ Performance (no boxing/dynamic dispatch)

**Common Client ID Types**:
- `String`: User IDs, API keys
- `IpAddr`: IP-based rate limiting  
- `u64`: High-performance numeric IDs
- Custom types: Complex client identification

## Future Extensibility

### Planned Architecture Enhancements

1. **Async Support**:
   ```rust
   async fn check_request_async(&self, client_id: T) -> Result<Decision, Error>
   ```
   - Non-blocking I/O integration
   - Async clock sources
   - Future-based cleanup

2. **Persistence Layer**:
   ```rust
   trait StateStore: Send + Sync {
       async fn load_state(&self, client_id: &T) -> Result<Option<u64>, Error>;
       async fn save_state(&self, client_id: &T, tat: u64) -> Result<(), Error>;
   }
   ```
   - Redis backend
   - Database persistence
   - Distributed state management

3. **Observability Integration**:
   ```rust
   trait RateLimiterMetrics {
       fn record_decision(&self, client_id: &T, allowed: bool, latency: Duration);
       fn record_error(&self, error: &FluxLimiterError);
   }
   ```
   - Prometheus metrics
   - OpenTelemetry traces
   - Custom metric backends

4. **Alternative Algorithms**:
   ```rust
   trait RateLimitAlgorithm<T> {
       fn check_request(&self, client_id: &T, current_time: u64) -> Decision;
   }
   ```
   - Token bucket implementation
   - Sliding window algorithms
   - Leaky bucket variants

### Extensibility Patterns

1. **Trait-Based Design**: Core algorithms as pluggable traits
2. **Generic Storage**: Abstract storage backend
3. **Middleware Pattern**: Composable rate limiting policies
4. **Plugin Architecture**: Runtime-configurable behaviors

### Backward Compatibility Strategy

- **Semantic Versioning**: Clear breaking change communication
- **Feature Flags**: Optional new functionality
- **Deprecation Path**: Gradual migration for breaking changes
- **Type Aliases**: Maintain existing API surface

## Conclusion

The Flux Limiter architecture prioritizes correctness, performance, and reliability through:

- **Mathematical Precision**: GCRA algorithm with nanosecond timing
- **Lock-Free Concurrency**: High-performance multi-threaded access
- **Comprehensive Error Handling**: Graceful degradation and recovery
- **Rich Observability**: Detailed metadata for monitoring
- **Flexible Design**: Generic client IDs and pluggable time sources
- **Robust Testing**: Deterministic test infrastructure

This architecture provides a solid foundation for high-performance rate limiting while maintaining the flexibility needed for diverse production environments.