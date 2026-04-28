# Concurrency Model

Understanding the lock-free concurrency design in Flux Limiter.

## Lock-Free Design Philosophy

Flux Limiter is built on a lock-free concurrency model for maximum performance:

```
Multiple Threads
      ↓
FluxLimiter (Shared via Arc)
      ↓
Arc<DashMap<ClientId, TAT>>
      ↓
Lock-free hash map operations
```

## Thread Safety Guarantees

### 1. Read Operations

Multiple concurrent readers without contention:

```rust
// Thread 1
limiter.check_request("client_1")?;

// Thread 2 (concurrent)
limiter.check_request("client_2")?;

// No lock contention - different shards
```

### 2. Write Operations

Atomic read-modify-write using DashMap's `entry()` API:

```rust
use dashmap::mapref::entry::Entry;

match client_state.entry(client_id) {
    Entry::Occupied(mut entry) => {
        let previous_tat = *entry.get();
        // ... compute new_tat ...
        entry.insert(new_tat);
    }
    Entry::Vacant(entry) => {
        entry.insert(new_tat);
    }
}
```

Each client's state is updated atomically — the read and write happen within a single
shard lock, preventing TOCTOU race conditions when the same client makes concurrent requests.

### 3. Memory Ordering

Relaxed ordering is sufficient for TAT updates:

- TAT values are monotonically increasing
- No cross-client dependencies
- No ABA problem

### 4. Concurrent Access Patterns

**Same Client, Different Threads**:
```rust
// Thread 1
limiter.check_request("client_1")?; // TAT = 100ms

// Thread 2 (concurrent)
limiter.check_request("client_1")?; // TAT = 200ms

// DashMap's entry() API holds the shard lock across
// the read and write, serializing same-client updates
// atomically and preventing TOCTOU races
```

**Different Clients, Different Threads**:
```rust
// Thread 1
limiter.check_request("client_1")?;

// Thread 2 (concurrent)
limiter.check_request("client_2")?;

// Lock-free - different shards, no contention
```

## DashMap: The Core Concurrency Primitive

### Why DashMap?

DashMap provides lock-free concurrent access through sharding:

```
DashMap<ClientId, TAT>
    ├─ Shard 0: HashMap + RwLock
    ├─ Shard 1: HashMap + RwLock
    ├─ Shard 2: HashMap + RwLock
    ├─ ...
    └─ Shard N: HashMap + RwLock
```

**Benefits**:
- **Better than std::HashMap + Mutex**: Avoids global locking
- **Better than RwLock**: No reader/writer contention
- **Better than custom lock-free map**: Mature, battle-tested
- **Segmented locking**: Reduces contention vs. single lock

### Sharding Strategy

```rust
// Client IDs are hashed to shards
shard_index = hash(client_id) % num_shards

// Different clients likely in different shards
// Same client always in same shard (consistency)
```

### Operations on DashMap

**Entry (Atomic Read-Modify-Write)**:
```rust
use dashmap::mapref::entry::Entry;

match client_state.entry(client_id) {
    Entry::Occupied(mut entry) => {
        let previous_tat = *entry.get();
        // ... compute new_tat ...
        entry.insert(new_tat);
        // Shard lock held across read + write
        // Prevents TOCTOU race conditions
    }
    Entry::Vacant(entry) => {
        entry.insert(new_tat);
    }
}
```

This ensures the conformance check and TAT update are atomic per-client,
preserving the GCRA invariant even under concurrent access.

**Cleanup (Iteration)**:
```rust
client_state.retain(|_, &mut tat| {
    // Locks each shard sequentially
    // Short lock duration per shard
    current_time - tat < threshold
});
```

## Memory Consistency

### Happens-Before Relationships

DashMap provides happens-before guarantees:

```rust
// Thread 1
limiter.check_request("client_1")?; // Writes TAT = 100ms

// Thread 2 (later)
limiter.check_request("client_1")?; // Reads TAT >= 100ms
```

The write in Thread 1 happens-before the read in Thread 2.

### Visibility Guarantees

All updates to client state are immediately visible:

```rust
// Thread 1: via entry() API
match client_state.entry("client_1") {
    Entry::Occupied(mut e) => { e.insert(100); }
    Entry::Vacant(e) => { e.insert(100); }
}

// Thread 2
if let Some(tat) = client_state.get("client_1") {
    // Sees 100
}
```

DashMap ensures proper memory synchronization.

## Sharing Across Threads

### Arc for Shared Ownership

```rust
use std::sync::Arc;

let config = FluxLimiterConfig::new(100.0, 50.0);
let limiter = Arc::new(FluxLimiter::with_config(config, SystemClock)?);

// Clone Arc for each thread (cheap)
let limiter1 = Arc::clone(&limiter);
let limiter2 = Arc::clone(&limiter);

// Use in different threads
thread::spawn(move || limiter1.check_request("client_1"));
thread::spawn(move || limiter2.check_request("client_2"));
```

**Arc Benefits**:
- Reference counting with atomic operations
- Thread-safe shared ownership
- Cheap to clone (increments counter)
- No data duplication

### Clone for Shared State

`FluxLimiter` implements `Clone` (when `C: Clone`), which shares the same
client state via `Arc::clone` on the internal `DashMap`:

```rust
let config = FluxLimiterConfig::new(100.0, 50.0);
let limiter = FluxLimiter::with_config(config, SystemClock)?;

// Clone the limiter directly — shares the same DashMap
let limiter1 = limiter.clone();
let limiter2 = limiter.clone();

// Both operate on the same client state
thread::spawn(move || limiter1.check_request("client_1"));
thread::spawn(move || limiter2.check_request("client_2"));
```

This provides the same shared-state semantics as `Arc<FluxLimiter<..>>`
without the extra indirection.

### Internal Arc Usage

```rust
pub struct FluxLimiter<T, C> {
    // ...
    client_state: Arc<DashMap<T, u64>>, // Shared across clones
    // ...
}
```

Cloning `FluxLimiter` shares the same state:

```rust
let limiter1 = limiter.clone();
let limiter2 = limiter.clone();

// Both share the same client_state
// Updates visible to both
```

## Contention Analysis

### Low Contention Scenarios

**Different clients, concurrent access**:
```
Threads: [T1: client_1] [T2: client_2] [T3: client_3]
Shards:  [Shard 0    ] [Shard 1    ] [Shard 2    ]
Result:  Lock-free, no contention
```

### Medium Contention Scenarios

**Same shard, different clients**:
```
Threads: [T1: client_1] [T2: client_100]
Shards:  [Shard 0    ] [Shard 0      ]
Result:  Some contention, but short lock duration
```

### High Contention Scenarios

**Same client, concurrent access**:
```
Threads: [T1: client_1] [T2: client_1] [T3: client_1]
Shards:  [Shard 0    ] [Shard 0    ] [Shard 0    ]
Result:  Serialized access (expected for consistency)
```

**This is correct behavior**: Rate limiting the same client from multiple threads should be serialized to maintain correctness.

## Performance Under Concurrency

### Scalability Characteristics

```
Threads  Throughput  Latency
1        1.0x        1.0μs
2        1.9x        1.0μs
4        3.7x        1.1μs
8        7.0x        1.2μs
16       12.5x       1.3μs
```

Nearly linear scalability with different clients.

### Benchmark: Concurrent Access

```rust
#[bench]
fn bench_concurrent_different_clients(b: &mut Bencher) {
    let limiter = Arc::new(FluxLimiter::with_config(
        FluxLimiterConfig::new(1000.0, 500.0),
        SystemClock
    ).unwrap());

    b.iter(|| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let limiter = Arc::clone(&limiter);
                thread::spawn(move || {
                    for j in 0..1000 {
                        let client_id = format!("client_{}_{}", i, j);
                        limiter.check_request(client_id).unwrap();
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    });
}
```

## Clock Thread Safety

The `Clock` trait requires `Send + Sync`:

```rust
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<u64, ClockError>;
}
```

**SystemClock**:
```rust
impl Clock for SystemClock {
    fn now(&self) -> Result<u64, ClockError> {
        // SystemTime::now() is thread-safe
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .map_err(|_| ClockError::SystemTimeError)
    }
}
```

**TestClock**:
```rust
pub struct TestClock {
    time: Arc<AtomicU64>,        // Atomic for thread safety
    should_fail: Arc<AtomicBool>, // Atomic for thread safety
}
```

Both implementations are thread-safe.

## Cleanup Concurrency

Cleanup can run concurrently with rate limiting:

```rust
// Thread 1: Rate limiting
limiter.check_request("client_1")?;

// Thread 2: Cleanup (concurrent)
limiter.cleanup_stale_clients(threshold)?;

// Thread 3: More rate limiting (concurrent)
limiter.check_request("client_2")?;
```

DashMap's `retain` method:
- Locks each shard briefly
- Other shards remain accessible
- Minimal impact on concurrent operations

## Best Practices

### 1. Share via Arc or Clone

```rust
// Option A: Share limiter across threads via Arc
let limiter = Arc::new(FluxLimiter::with_config(config, clock)?);

// Option B: Clone the limiter directly (shares the same DashMap)
let limiter = FluxLimiter::with_config(config, clock)?;
let limiter2 = limiter.clone();

// Bad: Create multiple limiters (separate state)
let limiter1 = FluxLimiter::with_config(config, clock1)?;
let limiter2 = FluxLimiter::with_config(config, clock2)?;
```

### 2. Use Different Client IDs

```rust
// Good: Different clients = low contention
for i in 0..1000 {
    limiter.check_request(format!("client_{}", i))?;
}

// Bad: Same client = serialized access
for _ in 0..1000 {
    limiter.check_request("same_client")?;
}
```

### 3. Avoid Blocking in Callbacks

```rust
// Good: Quick check
match limiter.check_request(client_id) {
    Ok(decision) => decision.allowed,
    Err(_) => false,
}

// Bad: Blocking I/O while holding client state
match limiter.check_request(client_id) {
    Ok(decision) => {
        // Don't do expensive work here
        database.log_decision(decision)?; // Blocks!
        decision.allowed
    }
    Err(_) => false,
}
```

### 4. Periodic Cleanup

```rust
// Background cleanup task
tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        let _ = limiter.cleanup_stale_clients(threshold);
    }
});
```

## Next Steps

- [Performance Design](./performance.md) - Optimization techniques
- [Testing Architecture](./testing.md) - Concurrent testing strategies
- [Design Decisions](./design-decisions.md) - Why these choices?
