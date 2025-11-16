# Flux Limiter

A high-performance rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free concurrent access.

## Features

- **Mathematically precise**: Implements the GCRA algorithm with exact nanosecond timing
- **High performance**: Lock-free concurrent access using DashMap
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Rich metadata**: Returns detailed decision information for HTTP response construction
- **Memory efficient**: Automatic cleanup of stale client entries
- **Robust error handling**: Graceful handling of clock failures and configuration errors
- **Testable**: Clock abstraction enables deterministic testing
- **Thread-safe**: Safe to use across multiple threads
- **Zero allocations**: Efficient hot path with minimal overhead

## What is Rate Limiting?

Rate limiting is a technique used to control the rate at which requests or operations are processed. It's commonly used to:

- **Protect services**: Prevent abuse and ensure fair resource allocation
- **Control costs**: Limit API usage to manage infrastructure costs
- **Ensure quality of service**: Prevent individual users from degrading performance for others
- **Comply with policies**: Enforce usage limits and SLA agreements

## Why Flux Limiter?

Flux Limiter stands out with its focus on:

1. **Correctness**: Mathematically precise GCRA implementation
2. **Performance**: Lock-free concurrency with O(1) operations
3. **Reliability**: Comprehensive error handling and graceful degradation
4. **Observability**: Rich metadata for monitoring and HTTP headers
5. **Flexibility**: Generic design supporting various client ID types

## Algorithm: GCRA

Flux Limiter implements the Generic Cell Rate Algorithm (GCRA), which is mathematically equivalent to the token bucket algorithm but offers several advantages:

- No background token refill processes
- Exact timing without floating-point precision loss
- Efficient state representation (one timestamp per client)
- Deterministic behavior with integer arithmetic

## Performance Characteristics

- **Memory**: O(number of active clients)
- **Time complexity**: O(1) for `check_request()` operations
- **Concurrency**: Lock-free reads and writes via DashMap
- **Precision**: Nanosecond timing accuracy
- **Throughput**: Millions of operations per second
- **Reliability**: Graceful degradation on system clock issues

## Next Steps

- [Installation](./getting-started/installation.md) - Add Flux Limiter to your project
- [Quick Start](./getting-started/quick-start.md) - Get started in minutes
- [Architecture](./architecture/overview.md) - Understand the design and implementation
