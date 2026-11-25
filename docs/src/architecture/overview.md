# Architecture Overview

This document provides a comprehensive overview of the Flux Limiter's architecture, design decisions, and implementation details.

## Design Philosophy

Flux Limiter is built on several key architectural principles:

1. **Fine-Grained Concurrency**: Each client's state is one atomic value, so updating a known client takes no lock
2. **Clock Abstraction**: Enables testing and handles time-related failures
3. **Type Safety**: Leverages Rust's type system for correctness guarantees
4. **Graceful Degradation**: Continues operation despite partial failures

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
│  ├─ DashMap<T, AtomicU64> │ Sharded map of per-client TATs  │
│  ├─ Atomic Operations     │ Thread-safe state updates       │
│  └─ Memory Management     │ Automatic cleanup mechanisms    │
├─────────────────────────────────────────────────────────────┤
│  Time Abstraction                                           │
│  ├─ Clock Trait           │ Pluggable time source           │
│  ├─ MonotonicClock        │ Production time (the default)   │
│  ├─ SystemClock           │ Wall-clock time, opt-in         │
│  └─ TestClock             │ Deterministic test time         │
└─────────────────────────────────────────────────────────────┘
```

## Data Flow

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

## Key Design Principles

### Performance First

- **O(1) operations**: Constant time complexity for rate limit checks
- **Fine-grained concurrency**: No global lock; requests for one client never wait on a lock
- **Nanosecond precision**: Integer arithmetic avoids floating-point overhead
- **No lock on the hot path**: updating a known client is a single compare-and-swap

### Correctness Guaranteed

- **Mathematical precision**: Exact GCRA implementation
- **Type safety**: Rust's type system prevents common errors
- **Comprehensive testing**: Deterministic tests with TestClock
- **No silent failures**: All errors are explicitly handled

### Observability Built-in

- **Rich metadata**: Every decision includes detailed information
- **Error transparency**: Clear error types and recovery paths
- **Memory tracking**: Built-in cleanup and monitoring capabilities
- **Production-ready**: Designed for monitoring and debugging

### Flexible and Extensible

- **Generic client IDs**: Works with any hashable type
- **Pluggable time sources**: Clock abstraction for testing
- **Composable**: Multiple limiters for complex scenarios
- **Future-proof**: Architecture supports future enhancements

## System Requirements

### Dependencies

- **DashMap**: Sharded concurrent hash map (one `RwLock` per shard)
- **Rust Standard Library**: Minimal external dependencies
- **No async runtime**: Works with any async runtime (Tokio, async-std, etc.)

### Performance Characteristics

- **Memory**: O(number of active clients)
- **Time complexity**: O(1) for check_request()
- **Concurrency**: Lock-free update of a known client; brief sharded locks for lookup and first insert
- **Precision**: Nanosecond timing accuracy
- **Throughput**: Depends on workload and hardware

### Scalability

```
Clients     Memory Usage
1K          ~48KB
100K        ~4.8MB
1M          ~48MB
10M         ~480MB
```

## Architecture Layers

### 1. API Layer

Public interface for rate limiting operations:
- `check_request()`: Primary rate limiting decision
- `cleanup_stale_clients()`: Memory management
- `rate()`, `burst()`: Configuration inspection

### 2. Algorithm Layer

GCRA implementation with nanosecond precision:
- Theoretical Arrival Time (TAT) calculation
- Conformance checking
- Metadata computation

### 3. Storage Layer

Thread-safe state management:
- DashMap for concurrent access
- Atomic operations for consistency
- Efficient memory layout

### 4. Time Abstraction Layer

Clock trait for time source flexibility:
- MonotonicClock for production (the default; immune to OS clock steps)
- SystemClock for wall-clock time, opt-in
- TestClock for deterministic testing
- Custom clocks for special scenarios

## Next Steps

- [GCRA Algorithm](./gcra-algorithm.md) - Understand the core algorithm
- [Component Design](./components.md) - Explore individual components
- [Concurrency Model](./concurrency.md) - Learn about thread safety
- [Performance Design](./performance.md) - Understand performance characteristics