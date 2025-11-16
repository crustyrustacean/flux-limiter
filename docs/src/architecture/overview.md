# Architecture Overview

This document provides a comprehensive overview of the Flux Limiter's architecture, design decisions, and implementation details.

## Design Philosophy

Flux Limiter is built on several key architectural principles:

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
- **Lock-free concurrency**: No global locks or contention
- **Nanosecond precision**: Integer arithmetic avoids floating-point overhead
- **Minimal allocations**: Hot path reuses existing memory

### Correctness Guaranteed

- **Mathematical precision**: Exact GCRA implementation
- **Type safety**: Rust's type system prevents common errors
- **Comprehensive testing**: >95% code coverage with deterministic tests
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

- **DashMap**: Lock-free concurrent hash map
- **Rust Standard Library**: Minimal external dependencies
- **No async runtime**: Works with any async runtime (Tokio, async-std, etc.)

### Performance Characteristics

- **Memory**: O(number of active clients)
- **Time complexity**: O(1) for check_request()
- **Concurrency**: Lock-free reads and writes
- **Precision**: Nanosecond timing accuracy
- **Throughput**: Millions of operations per second

### Scalability

```
Clients     Memory Usage    Latency
1K          ~32KB          < 1μs
100K        ~3.2MB         < 1μs
1M          ~32MB          < 1μs
10M         ~320MB         < 1μs
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
- SystemClock for production
- TestClock for deterministic testing
- Custom clocks for special scenarios

## Next Steps

- [GCRA Algorithm](./gcra-algorithm.md) - Understand the core algorithm
- [Component Design](./components.md) - Explore individual components
- [Concurrency Model](./concurrency.md) - Learn about thread safety
- [Performance Design](./performance.md) - Understand performance characteristics
