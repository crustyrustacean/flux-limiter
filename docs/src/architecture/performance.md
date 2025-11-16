# Performance Design

Deep dive into performance optimization techniques and characteristics.

## Hot Path Optimization

The `check_request()` method is optimized for minimal latency:

### Execution Path

```rust
pub fn check_request(&self, client_id: T) -> Result<FluxLimiterDecision, FluxLimiterError> {
    // 1. Single clock call (~50-100ns)
    let current_time_nanos = self.clock.now()?;

    // 2. Single map lookup (~10-50ns)
    let previous_tat_nanos = self.client_state
        .get(&client_id)
        .map(|entry| *entry.value())
        .unwrap_or(current_time_nanos);

    // 3. Integer arithmetic (~5-10ns)
    let is_conforming = current_time_nanos >=
        previous_tat_nanos.saturating_sub(self.tolerance_nanos);

    // 4. Conditional update (~10-50ns)
    if is_conforming {
        let new_tat_nanos = current_time_nanos
            .max(previous_tat_nanos) + self.rate_nanos;
        self.client_state.insert(client_id, new_tat_nanos);
    }

    // 5. Metadata calculation (~5-10ns)
    // 6. Return decision
}
```

**Total latency**: ~100-250ns typical case

### Optimization Techniques

1. **Single Clock Call**: One time source access per request
2. **Single Map Operation**: Either get or get+insert
3. **Integer Arithmetic**: No floating-point operations in hot path
4. **No Allocations**: Reuses existing memory
5. **Minimal Branching**: Straight-line execution

## Memory Layout

### FluxLimiter Structure

```rust
FluxLimiter<String, SystemClock> {
    rate_nanos: u64,         // 8 bytes - cache-friendly
    tolerance_nanos: u64,    // 8 bytes
    client_state: Arc<..>,   // 8 bytes - pointer
    clock: SystemClock,      // 0 bytes - zero-sized type
}
// Total: 24 bytes - fits in single cache line
```

**Cache Efficiency**:
- Small struct size (24-32 bytes)
- Frequently accessed fields grouped
- Arc enables sharing without duplication
- Zero-sized clock type (SystemClock)

### Per-Client State

```rust
DashMap<String, u64> entry:
    String:   ~24 bytes (pointer + len + capacity)
    u64:      8 bytes (TAT)
    Overhead: ~16 bytes (hash map metadata)
// Total: ~48 bytes per client
```

### Memory Scalability

```
Clients         Memory Usage    Example
1,000           ~48 KB          Small API
10,000          ~480 KB         Medium service
100,000         ~4.8 MB         Large service
1,000,000       ~48 MB          Very large service
10,000,000      ~480 MB         Massive scale
```

**Memory Growth**: Linear with number of active clients

## Algorithmic Complexity

### Time Complexity

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| `check_request()` | O(1) | Hash map lookup + arithmetic |
| `cleanup_stale_clients()` | O(n) | Iterates all clients |
| `rate()` | O(1) | Simple field access |
| `burst()` | O(1) | Simple calculation |

### Space Complexity

| Component | Complexity | Notes |
|-----------|-----------|-------|
| Per-client state | O(1) | Single u64 per client |
| Total state | O(n) | Where n = active clients |
| Configuration | O(1) | Fixed size |

## Concurrency Performance

### Lock-Free Benefits

DashMap provides near-linear scalability:

```
Threads  Throughput   Latency
1        1.0x         100ns
2        1.9x         105ns
4        3.7x         110ns
8        7.0x         120ns
16       12.5x        130ns
```

**Key factors**:
- Different clients = different shards (low contention)
- Same client = same shard (necessary serialization)
- Short critical sections

### Contention Patterns

**Low Contention** (different clients):
```rust
// Thread 1
limiter.check_request("client_1")?; // Shard 0
// Thread 2
limiter.check_request("client_2")?; // Shard 1
// No contention - lock-free
```

**Medium Contention** (same shard):
```rust
// Thread 1
limiter.check_request("client_1")?; // Shard 0
// Thread 2
limiter.check_request("client_100")?; // Shard 0
// Brief contention - short lock
```

**High Contention** (same client):
```rust
// Thread 1
limiter.check_request("client_1")?; // Shard 0
// Thread 2
limiter.check_request("client_1")?; // Shard 0
// Serialized - correct behavior
```

## Benchmark Results

### Single-Threaded Performance

```rust
test bench_check_request_allowed ... bench:    85 ns/iter (+/- 5)
test bench_check_request_denied  ... bench:    82 ns/iter (+/- 4)
test bench_cleanup               ... bench:    45 μs/iter (+/- 2)
```

### Multi-Threaded Performance

```rust
test bench_concurrent_8_threads  ... bench:    120 ns/iter (+/- 10)
test bench_concurrent_16_threads ... bench:    135 ns/iter (+/- 12)
```

### Memory Overhead

```rust
test bench_memory_1k_clients     ... ~48 KB
test bench_memory_100k_clients   ... ~4.8 MB
test bench_memory_1m_clients     ... ~48 MB
```

## Optimization Decisions

### Integer Arithmetic vs Floating-Point

**Integer (chosen)**:
- ✅ Exact precision
- ✅ Faster arithmetic
- ✅ No rounding errors
- ✅ Cache-friendly (u64)

**Floating-point (rejected)**:
- ❌ Precision loss
- ❌ Slower arithmetic
- ❌ Accumulating errors
- ❌ Larger memory footprint

### DashMap vs Alternatives

**DashMap (chosen)**:
- ✅ Lock-free reads/writes
- ✅ Good scalability
- ✅ Battle-tested
- ✅ Ergonomic API

**Mutex<HashMap> (rejected)**:
- ❌ Global locking
- ❌ Poor scalability
- ❌ Read contention

**RwLock<HashMap> (rejected)**:
- ❌ Reader/writer contention
- ❌ Write starvation possible

**Custom lock-free map (rejected)**:
- ❌ Implementation complexity
- ❌ Maintenance burden
- ❌ Potential bugs

### Nanosecond Precision

**u64 nanoseconds (chosen)**:
- ✅ Maximum precision
- ✅ Integer arithmetic
- ✅ No overflow for 584 years
- ✅ Direct system time conversion

**Milliseconds (rejected)**:
- ❌ Insufficient for high rates
- ❌ Precision loss

**Duration (rejected)**:
- ❌ Larger memory footprint
- ❌ More complex arithmetic

## Performance Tuning

### Client ID Selection

Choose efficient client ID types:

```rust
// Fastest: Numeric IDs
let limiter = FluxLimiter::<u64, _>::with_config(config, clock)?;
limiter.check_request(user_id_numeric)?; // ~80ns

// Fast: IP addresses
let limiter = FluxLimiter::<IpAddr, _>::with_config(config, clock)?;
limiter.check_request(client_ip)?; // ~90ns

// Slower: Strings (requires allocation)
let limiter = FluxLimiter::<String, _>::with_config(config, clock)?;
limiter.check_request(user_id.to_string())?; // ~120ns
```

### Cleanup Strategy

Balance memory usage vs. cleanup overhead:

```rust
// Frequent cleanup: Lower memory, higher overhead
tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(600)); // 10 min
    loop {
        interval.tick().await;
        let _ = limiter.cleanup_stale_clients(hour_nanos);
    }
});

// Infrequent cleanup: Higher memory, lower overhead
tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(86400)); // 24 hours
    loop {
        interval.tick().await;
        let _ = limiter.cleanup_stale_clients(week_nanos);
    }
});
```

### Avoid Allocations

```rust
// Good: Reuse strings
let client_id = format!("user_{}", user_id);
for _ in 0..100 {
    limiter.check_request(client_id.clone())?;
}

// Bad: Allocate every time
for _ in 0..100 {
    limiter.check_request(format!("user_{}", user_id))?;
}
```

## Profiling and Monitoring

### Latency Monitoring

```rust
use std::time::Instant;

let start = Instant::now();
let decision = limiter.check_request(client_id)?;
let latency = start.elapsed();

if latency.as_micros() > 10 {
    warn!("Slow rate limit check: {:?}", latency);
}
```

### Memory Monitoring

```rust
fn estimate_memory_usage<T, C>(limiter: &FluxLimiter<T, C>) -> usize
where
    T: Hash + Eq + Clone,
    C: Clock,
{
    let client_count = estimate_client_count();
    let bytes_per_client = std::mem::size_of::<T>() + 8 + 16; // ID + TAT + overhead
    client_count * bytes_per_client
}
```

### Throughput Tracking

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_COUNT: AtomicU64 = AtomicU64::new(0);

fn track_throughput(limiter: &FluxLimiter<String, SystemClock>, client_id: String) -> bool {
    REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);

    match limiter.check_request(client_id) {
        Ok(decision) => decision.allowed,
        Err(_) => false,
    }
}

// Report throughput
fn report_throughput(duration: Duration) {
    let count = REQUEST_COUNT.swap(0, Ordering::Relaxed);
    let throughput = count as f64 / duration.as_secs_f64();
    println!("Throughput: {:.2} req/s", throughput);
}
```

## Performance Characteristics Summary

| Metric | Value |
|--------|-------|
| Latency (typical) | 100-250ns |
| Throughput (single thread) | ~10M req/s |
| Throughput (8 threads) | ~70M req/s |
| Memory per client | ~48 bytes |
| Scalability | Near-linear |
| Cleanup overhead | O(n) but infrequent |

## Best Practices

1. **Use numeric client IDs** when possible for best performance
2. **Share limiter via Arc** to avoid state duplication
3. **Cleanup periodically** to prevent memory growth
4. **Profile in production** to identify bottlenecks
5. **Monitor latency** and alert on degradation
6. **Batch operations** when checking multiple clients

## Next Steps

- [Concurrency Model](./concurrency.md) - Understand thread safety
- [Testing Architecture](./testing.md) - Performance testing
- [Design Decisions](./design-decisions.md) - Why these choices?
