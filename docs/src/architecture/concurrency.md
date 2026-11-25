# Concurrency Model

Understanding the concurrency design in Flux Limiter.

## Design in One Picture

Flux Limiter keeps one atomic value per client inside a sharded map. Updating a
known client takes no lock at all; the map's shard lock is taken only when a
client is seen for the first time.

```
Multiple Threads
      ↓
FluxLimiter (Shared via Arc or Clone)
      ↓
Arc<DashMap<ClientId, AtomicU64>>     one TAT per client
      ↓
  known client   → load, decide, compare-and-swap   (no lock)
  new client     → insert under the shard lock      (once per client)
```

The word "lock-free" is used here in a precise sense. An update to a client
that is already tracked takes no lock: it retries instead of waiting, so it
makes progress as long as the thread running it gets to run. The map itself is
built from sharded `RwLock`s, so finding a client and adding one the first time
do take a brief lock on one shard.

There is one deliberate exception. A request that loses the compare-and-swap 64
times in a row stops retrying and settles under the shard's write lock, where
it waits for that lock. This only happens under extreme contention on a single
client, and it is there so that a hot client cannot make a request retry
forever.

## Thread Safety Guarantees

### 1. Read Operations

Multiple concurrent readers without contention:

```rust
// Thread 1
limiter.check_request("client_1")?;

// Thread 2 (concurrent)
limiter.check_request("client_2")?;

// Both take the fast path: no lock is held while deciding
```

### 2. Write Operations

A compare-and-swap on the client's atomic TAT. The new TAT is published only if
the TAT is still the one the decision was computed from:

```rust
let previous_tat = tat.load(Ordering::Relaxed);
let decision = decide(previous_tat, now);          // pure: reads no shared state
if decision.allowed {
    tat.compare_exchange(
        previous_tat,                               // only if unchanged
        decision.reset_time_nanos,
        Ordering::Relaxed,
        Ordering::Relaxed,
    )?;                                             // otherwise: retry
}
```

A request that loses the race (another request advanced the TAT first) retries
from the new value, so each allowed request advances the TAT by exactly one
interval. No update can be lost, which is what prevents the TOCTOU race
conditions that a plain read-then-write would allow when the same client makes
concurrent requests.

A client's first request is different: it has no TAT to compare against, so it is
inserted under the shard's write lock through DashMap's `entry()` API. If two
requests race to insert the same new client, the loser finds the entry occupied
and rejoins the compare-and-swap path rather than being granted a second "first"
request.

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

// Each request publishes its TAT with a compare-and-swap.
// The one that loses retries on the winner's value, so
// the TAT ends at 300ms, never back at 200ms.
```

**Different Clients, Different Threads**:
```rust
// Thread 1
limiter.check_request("client_1")?;

// Thread 2 (concurrent)
limiter.check_request("client_2")?;

// Two different atomics: the updates are independent.
// They only queue if both clients hash to the same shard
// *and* one of them is being inserted for the first time.
```

## DashMap: The Core Concurrency Primitive

### Why DashMap?

DashMap provides concurrent access through sharding. Each shard is a plain
`HashMap` behind an `RwLock`, so a lookup takes a shared read lock on one shard
and an insert takes that shard's write lock:

```
DashMap<ClientId, AtomicU64>
    ├─ Shard 0: HashMap + RwLock
    ├─ Shard 1: HashMap + RwLock
    ├─ Shard 2: HashMap + RwLock
    ├─ ...
    └─ Shard N: HashMap + RwLock
```

**Why the per-client atomic sits on top of it**:
- The shard lock is only needed to find or add a client, which is a small
  fraction of the work
- The GCRA decision and the TAT update, which happen on every request, never
  hold a lock
- Without the atomic, every request would need the shard's *write* lock, so
  every client in a shard would serialize behind every other
- **Better than std::HashMap + Mutex**: One stuck shard does not stall the rest
- **Better than custom lock-free map**: Mature, battle-tested

### Sharding Strategy

```rust
// Client IDs are hashed to shards
shard_index = hash(client_id) % num_shards

// Different clients likely in different shards
// Same client always in same shard (consistency)
```

### Operations on DashMap

**Update a known client (compare-and-swap, no lock)**:
```rust
if let Some(tat) = client_state.get(&client_id) {
    // `tat` is a Ref<Entry to AtomicU64>: a shared read lock on the shard.
    let previous = tat.load(Ordering::Relaxed);
    let decision = decide(previous, now);
    if decision.allowed {
        tat.compare_exchange(previous, decision.reset_time_nanos, ...)?;
    }
}
```

**Insert a new client (shard write lock, once per client)**:
```rust
match client_state.entry(client_id) {
    Entry::Vacant(entry) => entry.insert(AtomicU64::new(now + interval)),
    Entry::Occupied(_) => { /* lost the race: use the update path above */ }
}
```

Together these keep the conformance check and TAT update atomic per client,
preserving the GCRA invariant even under concurrent access.

**Cleanup (Iteration)**:
```rust
client_state.retain(|_, tat| {
    // Locks each shard sequentially
    // Short lock duration per shard
    tat.load(Ordering::Relaxed) + tolerance > cutoff
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
// Thread 1: compare-and-swap publishes the new TAT
tat.compare_exchange(99, 100, Ordering::Relaxed, Ordering::Relaxed)?;

// Thread 2, afterwards
if let Some(tat) = client_state.get("client_1") {
    tat.load(Ordering::Relaxed); // 100
}
```

`Relaxed` is enough here because the TAT is a single self-contained value with
no other memory it needs to publish. "Afterwards" is guaranteed by the
compare-and-swap itself: a request that reads 99 and tries to write 100 fails
if anyone else already moved the TAT on.

## Sharing Across Threads

### Arc for Shared Ownership

```rust
use std::sync::Arc;

let config = FluxLimiterConfig::new(100.0, 50.0);
let limiter = Arc::new(FluxLimiter::with_config(config, MonotonicClock::new())?);

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
let limiter = FluxLimiter::with_config(config, MonotonicClock::new())?;

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
    client_state: Arc<DashMap<T, AtomicU64>>, // Shared across clones
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
Result:  Independent atomics, shared read locks only
```

### Medium Contention Scenarios

**Same shard, different clients**:
```
Threads: [T1: client_1] [T2: client_100]
Shards:  [Shard 0    ] [Shard 0      ]
Result:  Shared read lock only; still independent atomics
```

### High Contention Scenarios

**Same client, concurrent access**:
```
Threads: [T1: client_1] [T2: client_1] [T3: client_1]
Shards:  [Shard 0    ] [Shard 0    ] [Shard 0    ]
TAT:     one atomic, three compare-and-swaps
Result:  No lock; requests that collide retry on the winner's value
```

**Correctness does not depend on serialization.** The invariant is that each
allowed request advances the TAT by one interval. A compare-and-swap enforces
that without a queue: whichever request wins moves the TAT, and the others
recompute against the new value.

## Performance Under Concurrency

### Scalability Characteristics

```
Different clients  → different atomics   (independent, scales well)
Same client        → one atomic         (compare-and-swap, no lock)
```

Requests for different clients never wait on each other's TAT. Requests for the
same client do not queue: each one that loses a compare-and-swap retries, and the
retry count rises with contention rather than the wait time.

### Benchmarking concurrent access

The repository has Criterion benchmarks for exactly these two cases. They
produced the figures in [Performance Design](./performance.md#concurrency-performance),
and they are the right way to check a change against them:

```sh
cargo bench contention
```

| Benchmark | What it measures |
|---|---|
| `contention_hot_key_u64` | 4 threads, all hitting one client: the worst case for same-client contention |
| `contention_spread_keys_u64` | 4 threads, each with its own clients: the common case |

Both are reported per request, with throughput in requests per second.

If you write your own, three things decide whether the number means anything:

- **Start the threads before the timed region.** Starting a thread costs tens of
  microseconds, which is hundreds of times the cost of a request. The
  repository's benchmarks start their workers once and release them on a
  channel for each timed round.
- **Make sure requests are allowed.** A denied request never touches the
  client's state, so a run that is mostly denials measures nothing about
  contention. Use a high rate so that every request writes.
- **Release the threads together.** Same-client requests only collide when they
  arrive at the same moment. Threads that start in a stagger take turns
  instead of contending, and the result looks far better than the worst case.

Rust's built-in `#[bench]` attribute is not stable and needs a nightly
toolchain, which is why the crate uses Criterion.

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

### 2. Know What a Hot Client Costs

```rust
// Independent clients: one atomic each, no interaction
for i in 0..1000 {
    limiter.check_request(format!("client_{}", i))?;
}

// One client from many threads: every request is a compare-and-swap on
// the same atomic. Correct, and lock-free, but the retries are real work:
limiter.check_request("same_client")?;
```

A single very hot client does not block anyone, but it is the one pattern where
contention is visible. It is also the pattern rate limiting exists for, so it is
never something to avoid on the limiter's account.

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