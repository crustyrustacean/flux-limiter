# Design Decisions

Detailed rationale behind key architectural choices in Flux Limiter.

## Algorithm Selection: GCRA vs Token Bucket

### Decision: Use GCRA

**Rationale**: The Generic Cell Rate Algorithm (GCRA) provides mathematical equivalence to Token Bucket with several implementation advantages.

### GCRA Advantages

✅ **Exact Mathematical Precision**
- Integer arithmetic with u64 nanoseconds
- No floating-point precision loss
- No accumulating rounding errors

✅ **Stateless Calculation**
- No background token refill processes
- No timer management overhead
- Simpler implementation

✅ **Efficient State**
- One timestamp (u64) per client
- vs. token count + last refill time
- Smaller memory footprint

✅ **Deterministic Behavior**
- Exact timing with integer arithmetic
- Predictable results in tests
- No timer drift

### Token Bucket Drawbacks

❌ **Floating-Point Precision**
- Token counts as floats
- Accumulating rounding errors
- Precision loss over time

❌ **Background Processes**
- Requires token refill mechanism
- Timer management complexity
- Potential drift issues

❌ **Complex State**
- Token count + timestamp
- More memory per client
- More complex updates

### Mathematical Equivalence

Both algorithms produce the same rate limiting behavior:

```
GCRA: Request allowed if current_time >= TAT - tolerance
Token Bucket: Request allowed if tokens >= 1
```

The difference is implementation, not behavior.

## Data Structure: DashMap

### Decision: Use DashMap for Client State

**Rationale**: DashMap provides the best balance of performance, correctness, and ergonomics.

### DashMap Benefits

✅ **Lock-Free Concurrency**
- Segmented locking
- Different shards = no contention
- Near-linear scalability

✅ **Battle-Tested**
- Mature library
- Well-documented
- Proven in production

✅ **Good Performance**
- ~100ns for get/insert
- Minimal overhead
- Efficient memory layout

✅ **Ergonomic API**
- Similar to std::HashMap
- Easy to use correctly
- Clear semantics

### Alternative: Mutex<HashMap>

❌ **Global Locking**
- All operations serialize
- Poor scalability
- Read contention

**Why rejected**: Unacceptable performance bottleneck for concurrent access.

### Alternative: RwLock<HashMap>

❌ **Reader/Writer Contention**
- Writers block all readers
- Write starvation possible
- Complex lock management

**Why rejected**: Still has contention issues, doesn't scale well.

### Alternative: Custom Lock-Free Map

❌ **Implementation Complexity**
- Requires deep concurrency expertise
- Bug-prone
- High maintenance burden

❌ **Maturity Risk**
- Unproven in production
- Potential correctness issues
- Performance unknowns

**Why rejected**: Not worth the complexity and risk when DashMap exists.

## Time Representation: Nanoseconds

### Decision: Use u64 Nanoseconds

**Rationale**: Nanoseconds provide maximum precision with efficient integer arithmetic.

### Benefits

✅ **Maximum Precision**
- Supports very high rates (1B req/s)
- No precision loss
- Exact calculations

✅ **Integer Arithmetic**
- Faster than floating-point
- No rounding errors
- Deterministic results

✅ **No Overflow**
- u64::MAX nanoseconds = 584 years
- Sufficient for any realistic usage
- Saturating arithmetic for safety

✅ **System Time Compatible**
- Direct conversion from SystemTime
- No conversion overhead
- Natural representation

### Alternative: Milliseconds

❌ **Insufficient Precision**
- Can't support rates > 1000 req/s precisely
- Precision loss for high rates
- Rounding errors

**Why rejected**: Limits maximum rate and precision.

### Alternative: Duration Type

❌ **Memory Overhead**
- Larger struct (2x u64)
- More complex arithmetic
- Cache inefficient

❌ **Performance**
- More expensive operations
- Additional abstractions
- Not necessary

**Why rejected**: Overhead without benefit for our use case.

### Alternative: f64 Seconds

❌ **Floating-Point Issues**
- Precision loss
- Rounding errors
- Non-deterministic

❌ **Performance**
- Slower arithmetic
- Cache less friendly
- Conversion overhead

**Why rejected**: Precision and performance concerns.

## Error Handling: Result Types

### Decision: Use Result for All Errors

**Rationale**: Explicit error handling enables graceful degradation and better observability.

### Benefits

✅ **Explicit Error Handling**
- Caller must handle errors
- No silent failures
- Clear error paths

✅ **Graceful Degradation**
- Application chooses policy
- Fail-open or fail-closed
- Fallback strategies possible

✅ **Better Observability**
- Errors can be logged
- Metrics can be collected
- Debugging easier

✅ **Production-Ready**
- Handles clock failures
- Recoverable errors
- Robust operation

### Alternative: Panics

❌ **Difficult Recovery**
- Can't recover from panic
- Crashes entire application
- Poor user experience

❌ **Poor Observability**
- Hard to monitor
- Difficult to debug
- No graceful degradation

❌ **Not Production-Ready**
- Unacceptable for library code
- Forces policy on users
- Fragile

**Why rejected**: Panics are inappropriate for library code and prevent graceful error handling.

### Alternative: Silent Failures (allow on error)

❌ **Hidden Bugs**
- Errors go unnoticed
- Hard to debug
- Incorrect behavior

❌ **No Observability**
- Can't track error rates
- No alerting possible
- Silent degradation

**Why rejected**: Silent failures make debugging impossible and hide problems.

## Generic Design: Client ID Types

### Decision: Generic Client ID (T: Hash + Eq + Clone)

**Rationale**: Generics provide flexibility while maintaining type safety and performance.

### Benefits

✅ **Flexibility**
- String: User IDs, API keys
- IpAddr: IP-based limiting
- u64: Numeric IDs
- Custom types: Complex scenarios

✅ **Zero-Cost Abstraction**
- No runtime overhead
- No boxing/dynamic dispatch
- Compile-time optimization

✅ **Type Safety**
- Compile-time type checking
- Can't mix different ID types
- Clear API

✅ **Performance**
- Monomorphization
- Inlined operations
- Optimal code generation

### Alternative: Trait Object (dyn ClientId)

❌ **Runtime Overhead**
- Dynamic dispatch
- Heap allocation (Box)
- Slower performance

❌ **Less Ergonomic**
- Requires trait implementations
- More verbose usage
- Lifetime complexity

**Why rejected**: Performance overhead without significant benefit.

### Alternative: String Only

❌ **Limited Flexibility**
- Forces string conversion
- Allocation overhead
- Can't optimize for numeric IDs

❌ **Performance**
- Always allocates
- Slower hash/compare
- Memory overhead

**Why rejected**: Generics provide flexibility without cost.

## Clock Abstraction

### Decision: Clock Trait

**Rationale**: Abstracting time enables testing and handles real-world clock issues.

### Benefits

✅ **Testable**
- TestClock for deterministic tests
- Precise time control
- Failure simulation

✅ **Handles Real-World Issues**
- Clock going backwards
- System suspend/resume
- Virtualization issues

✅ **Flexible**
- Custom time sources
- Mock implementations
- Production/test separation

✅ **Error Handling**
- Graceful clock failures
- Explicit error types
- Recoverable

### Alternative: Direct SystemTime

❌ **Not Testable**
- Can't control time in tests
- Non-deterministic tests
- Hard to test edge cases

❌ **No Error Handling**
- SystemTime can fail
- Can't recover
- Silent failures

**Why rejected**: Testing and error handling require abstraction.

## Memory Management: Manual Cleanup

### Decision: Explicit cleanup_stale_clients()

**Rationale**: Manual cleanup gives users control over memory/CPU tradeoff.

### Benefits

✅ **User Control**
- Choose cleanup frequency
- Choose threshold
- Tune for workload

✅ **No Background Threads**
- Simpler implementation
- No thread overhead
- User controls when

✅ **Predictable Performance**
- Cleanup only when called
- No surprising pauses
- Controllable overhead

### Alternative: Automatic Background Cleanup

❌ **Thread Overhead**
- Requires background thread
- Thread management complexity
- Resource usage

❌ **Loss of Control**
- Can't control timing
- Fixed thresholds
- May not fit all workloads

❌ **Complexity**
- Thread lifecycle management
- Shutdown coordination
- Error handling

**Why rejected**: Manual cleanup is simpler and more flexible.

### Alternative: Reference Counting

❌ **Incorrect Semantics**
- Clients don't have clear ownership
- Memory leak if client keeps requesting
- Doesn't solve problem

**Why rejected**: Doesn't match use case.

## Configuration: Builder Pattern

### Decision: Support Builder Pattern

**Rationale**: Builder pattern provides ergonomic API while maintaining validation.

### Benefits

✅ **Ergonomic**
- Clear, readable code
- Method chaining
- Self-documenting

✅ **Flexible**
- Can set individual fields
- Start with defaults
- Override as needed

✅ **Validation**
- Validate at build time
- Clear error messages
- Fail fast

### Example

```rust
let config = FluxLimiterConfig::new(0.0, 0.0)
    .rate(100.0)
    .burst(50.0);
```

vs.

```rust
let config = FluxLimiterConfig::new(100.0, 50.0);
```

Both are supported.

## Decision Summary

| Decision | Chosen | Rejected | Rationale |
|----------|--------|----------|-----------|
| Algorithm | GCRA | Token Bucket | Precision, simplicity |
| Storage | DashMap | Mutex, RwLock | Performance, scalability |
| Time | u64 nanos | ms, Duration, f64 | Precision, performance |
| Errors | Result | Panic, Silent | Observability, recovery |
| Client ID | Generic | Trait object, String | Flexibility, performance |
| Clock | Trait | Direct SystemTime | Testing, error handling |
| Cleanup | Manual | Automatic | Control, simplicity |
| Config | Builder | Constructor only | Ergonomics |

## Next Steps

- [Future Extensibility](./future.md) - Planned enhancements
- [Performance Design](./performance.md) - Performance implications
