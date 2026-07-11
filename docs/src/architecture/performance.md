# Performance Design

Deep dive into performance optimization techniques and characteristics.

## Hot Path Optimization

The `check_request()` method is optimized for minimal latency:

### Execution Path

```rust
pub fn check_request(&self, client_id: T) -> Result<FluxLimiterDecision, FluxLimiterError> {
    // 1. Single clock call
    let current_time_nanos = self.clock.now()?;

    // 2. Atomic read-modify-write via entry()
    //    Shard lock held across read + write, preventing TOCTOU races
    match self.client_state.entry(client_id) {
        Entry::Occupied(mut occupied) => {
            let previous_tat_nanos = *occupied.get();

            // 3. Integer arithmetic
            let is_conforming = current_time_nanos >=
                previous_tat_nanos.saturating_sub(self.tolerance_nanos);

            // 4. Conditional update (within same shard lock)
            if is_conforming {
                let new_tat_nanos = current_time_nanos
                    .max(previous_tat_nanos) + self.rate_nanos;
                occupied.insert(new_tat_nanos);
            }
            // ... return decision
        }
        Entry::Vacant(vacant) => {
            // First request from this client — always allowed
            let new_tat_nanos = current_time_nanos + self.rate_nanos;
            vacant.insert(new_tat_nanos);
            // ... return allowed decision
        }
    }

    // 5. Metadata calculation
    // 6. Return decision
}
```

The hot path involves a single clock call, a single DashMap entry operation, and simple integer arithmetic — all O(1).

### Optimization Techniques

1. **Single Clock Call**: One time source access per request
2. **Single Atomic Entry Operation**: `entry()` handles lookup + update within one shard lock
3. **Integer Arithmetic**: No floating-point operations in hot path
4. **Minimal Branching**: Straight-line execution

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

DashMap uses segmented locking for concurrent access:

```
Different clients → different shards (low contention)
Same client     → same shard (necessary serialization)
Short critical sections → minimal lock duration
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
// Numeric IDs avoid String allocation overhead
let limiter = FluxLimiter::<u64, _>::with_config(config, clock)?;
limiter.check_request(user_id_numeric)?;

// IP addresses
let limiter = FluxLimiter::<IpAddr, _>::with_config(config, clock)?;
limiter.check_request(client_ip)?;

// Strings (convenient but involves allocation per request)
let limiter = FluxLimiter::<String, _>::with_config(config, clock)?;
limiter.check_request(user_id.to_string())?;
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
| Memory per client | ~48 bytes |
| Time complexity | O(1) |
| Space complexity | O(n) |
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