# Performance Design

Deep dive into performance optimization techniques and characteristics.

## Hot Path Optimization

The `check_request()` method is optimized for minimal latency:

### Execution Path

```rust
pub fn check_request(&self, client_id: T) -> Result<FluxLimiterDecision, FluxLimiterError> {
    // 1. Single clock call
    let current_time_nanos = self.clock.now()?;

    // 2. Look up the client (shared read lock on one shard)
    if let Some(tat) = self.client_state.get(&client_id) {
        let previous_tat_nanos = tat.load(Ordering::Relaxed);

        // 3. Integer arithmetic
        let is_conforming = current_time_nanos >=
            previous_tat_nanos.saturating_sub(self.tolerance_nanos);

        // 4. Publish the new TAT, only if unchanged (no lock)
        if is_conforming {
            let new_tat_nanos = current_time_nanos
                .max(previous_tat_nanos)
                .saturating_add(self.rate_nanos);
            tat.compare_exchange(previous_tat_nanos, new_tat_nanos, ...)?;
        }
        // ... return decision
    } else {
        // First request from this client — always allowed (shard write lock)
        // ... insert initial TAT, return allowed decision
    }

    // 5. Metadata calculation
    // 6. Return decision
}
```

The hot path involves a single clock call, a single map lookup, one atomic
compare-and-swap, and simple integer arithmetic — all O(1).

### Optimization Techniques

1. **Single Clock Call**: One time source access per request
2. **No Lock on the Update**: a compare-and-swap publishes the new TAT, so requests for the same client do not queue behind a shard's write lock
3. **Integer Arithmetic**: No floating-point operations in hot path
4. **Minimal Branching**: Straight-line execution

## Memory Layout

### FluxLimiter Structure

```rust
FluxLimiter<String, MonotonicClock> {   // 64 bytes in total
    rate_nanos: u64,         // 8 bytes
    tolerance_nanos: u64,    // 8 bytes
    client_state: Arc<..>,   // 8 bytes (pointer)
    clock: MonotonicClock,   // 40 bytes - see below
}
```

**Size** (measured with `size_of` on a 64-bit target):

| Type | Size |
|------|------|
| `FluxLimiter<T, MonotonicClock>` | 64 bytes |
| `FluxLimiter<T, SystemClock>` | 24 bytes |
| `MonotonicClock` | 40 bytes |
| `SystemClock` | 0 bytes |

The size does not depend on the client ID type: `String` and `u64` keys both give
64 bytes, because the key lives in the map, not in the limiter.

**Cache Efficiency**:
- The limiter with the default clock is exactly one 64-byte cache line
- Frequently accessed fields are grouped at the start of the struct
- `Arc` enables sharing without duplication
- The clock is 40 bytes: an `Arc` to the shared anchor, an `Instant` origin, and a
  lazily-read epoch. That is why the struct grew from 24 bytes when the zero-sized
  `SystemClock` was the default. Pass `SystemClock` explicitly if those 40 bytes
  matter and you accept its limitation

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

### Memory Scalability

Measured with `u64` keys on a 64-bit target:

| Clients | Memory | Example |
|---|---|---|
| 1,000 | 46 KB | Small API |
| 10,000 | 0.3 MB | Medium service |
| 100,000 | 2.1 MB | Large service |
| 1,000,000 | 34 MB | Very large service |
| 10,000,000 | 272 MB | Massive scale |

Short `String` keys take about two and a half to three times as much.

**Memory Growth**: linear in the number of active clients on average, but not
smoothly. The client map doubles its tables when they fill, so memory holds
flat for a while and then steps up. At any one client count the true figure is
between about two thirds and the whole of a step.

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

### Where the Locks Are

DashMap is sharded: each shard is a `HashMap` behind an `RwLock`. On top of it,
each client's TAT is an `AtomicU64`. The two together decide what locks:

```
Different clients → different shards (low contention)
Same client     → one atomic  (compare-and-swap, no lock)
Short critical sections → minimal lock duration
```

**What takes a lock, and what does not**:
- Updating a known client: no lock (compare-and-swap on its atomic)
- Looking up a client: that shard's read lock, shared with other lookups
- Inserting a client for the first time: that shard's write lock, once per client
- `cleanup_stale_clients`: each shard's write lock in turn

Measured with 4 threads on one client (i7-13700HX, criterion, 1,000 requests per
iteration): 159 ns per request before the atomic TAT, and 67 to 96 ns per request
after it, over nine runs. That makes it roughly 1.6 to 2.4 times faster. The figure
moves by about 40% between runs, which happens when threads contend for one key on
a laptop, so treat the range as the result and not any single number in it. Four
threads on distinct clients take 31 ns per request and stay within 3% between runs,
since they were never waiting on the same lock to begin with.

### Contention Patterns

**Low Contention** (different clients):
```rust
// Thread 1
limiter.check_request("client_1")?; // Shard 0
// Thread 2
limiter.check_request("client_2")?; // Shard 1
// Independent: each has its own atomic
```

**Medium Contention** (same shard):
```rust
// Thread 1
limiter.check_request("client_1")?; // Shard 0
// Thread 2
limiter.check_request("client_100")?; // Shard 0
// Different atomics: the updates are independent. They only share a
// brief read lock on the shard while being looked up.
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
- ✅ Sharded locks, so one busy shard does not stall the others
- ✅ With an atomic per client, the per-request update needs no lock at all
- ✅ Battle-tested
- ✅ Ergonomic API

**`Mutex<HashMap>` (rejected)**:
- ❌ Global locking
- ❌ Poor scalability
- ❌ Read contention

**`RwLock<HashMap>` (rejected)**:
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

fn track_throughput(limiter: &FluxLimiter<String, MonotonicClock>, client_id: String) -> bool {
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
| Memory per client | ~31 bytes with a `u64` key, ~76-95 with a short `String` key (average over 1M+ clients; grows in steps) |
| Time complexity | O(1) |
| Space complexity | O(n) |
| Cleanup overhead | O(n) but infrequent |

## Best Practices

1. **Use numeric client IDs** when possible for best performance
2. **Clone the limiter or share it via `Arc`** rather than building one per thread; clones share the same clients, configuration and clock
3. **Cleanup periodically** to prevent memory growth
4. **Profile in production** to identify bottlenecks
5. **Monitor latency** and alert on degradation
6. **Batch operations** when checking multiple clients

## Next Steps

- [Concurrency Model](./concurrency.md) - Understand thread safety
- [Testing Architecture](./testing.md) - Performance testing
- [Design Decisions](./design-decisions.md) - Why these choices?