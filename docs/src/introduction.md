# Flux Limiter

A simple rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free per-client updates.

## Features

- **Precise**: Implements the GCRA algorithm in integer nanoseconds. The rate is rounded to a whole-nanosecond interval, which is exact for any rate that divides one billion
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Rich metadata**: Returns detailed decision information for HTTP response construction
- **Unaffected by clock changes**: The default clock measures elapsed time, so an operating-system clock correction cannot lock clients out
- **Memory efficient**: One atomic timestamp per client. Idle clients are removed when you call `cleanup_stale_clients()`; nothing runs in the background
- **Robust error handling**: Invalid configuration is rejected with an error, and the default clock cannot fail
- **Testable**: Clock abstraction enables deterministic testing
- **Thread-safe**: Safe to use across multiple threads

## What is Rate Limiting?

Rate limiting is a technique used to control the rate at which requests or operations are processed. It's commonly used to:

- **Protect services**: Prevent abuse and ensure fair resource allocation
- **Control costs**: Limit API usage to manage infrastructure costs
- **Ensure quality of service**: Prevent individual users from degrading performance for others
- **Comply with policies**: Enforce usage limits and SLA agreements

## Why Flux Limiter?

Flux Limiter stands out with its focus on:

1. **Correctness**: GCRA in integer arithmetic, with no floating-point state
2. **Performance**: O(1) operations, and updating a known client takes no lock
3. **Reliability**: Invalid configuration is reported, and rate limiting does not depend on the operating-system clock being correct
4. **Observability**: Rich metadata for monitoring and HTTP headers
5. **Flexibility**: Generic design supporting various client ID types

## Alternatives

Flux Limiter is one of several rate limiters on crates.io. These are the others we know of, described by what they do. Each claim below was checked against the published version at the time of writing.

- [`governor`](https://docs.rs/governor): GCRA with one 64-bit value of state per limiter, updated by compare-and-swap.
  - keyed limiters for many clients, and checks that cost more than one cell
  - can wait asynchronously until a request is allowed (`until_ready`, `until_key_ready`)
  - denial reports how long to wait, as a relative `Duration`
  - pluggable clock, with five shipped, including one you control in tests
  - optional `no_std`
- [`leaky-bucket`](https://docs.rs/leaky-bucket): a token bucket for async code.
  - tokens, a refill interval, and how many tokens are refilled per interval
  - `acquire(n).await` suspends the caller until `n` tokens are available
  - `try_acquire(n)` answers with a `bool`: no wait time, no remaining count
  - one limiter is one bucket; there is no per-client key
  - requires a Tokio runtime
- [`quota`](https://docs.rs/quota): a pool of per-key GCRA states, like Flux Limiter.
  - keys are passed by reference, and a check can cost more than one token
  - the result reports how much quota remains, but says nothing about time: no wait duration and no reset time
  - built on `quanta` for its clock

Where Flux Limiter differs:

- Every check returns the seconds to wait and the time the limit resets, as a Unix-epoch timestamp. None of the three report a reset time.
- It has no runtime requirement and links 11 crates in total. `leaky-bucket` requires Tokio.
- The key can be passed by reference (`check_request_ref`), as with `governor` and `quota`, so a `String` key is not cloned per request.

What it does not have:

- Weighted checks, async waiting, or `no_std`. `governor` has all three.

## Algorithm: GCRA

Flux Limiter implements the Generic Cell Rate Algorithm (GCRA), which is mathematically equivalent to the token bucket algorithm but offers several advantages:

- No background token refill processes
- No floating-point state: once the rate is a whole-nanosecond interval, the arithmetic is exact
- Efficient state representation (one timestamp per client)
- Deterministic behavior with integer arithmetic

## Performance Characteristics

- **Memory**: O(number of active clients)
- **Time complexity**: O(1) for `check_request()` operations
- **Concurrency**: Each client's state is a single atomic in a sharded `DashMap`; updating a known client is a compare-and-swap with no lock
- **Precision**: Nanosecond resolution. The rate interval is rounded to a whole number of nanoseconds
- **Reliability**: The default clock is monotonic, so a backwards clock step does not deny requests

## Next Steps

- [Installation](./getting-started/installation.md) - Add Flux Limiter to your project
- [Quick Start](./getting-started/quick-start.md) - Get started in minutes
- [Architecture](./architecture/overview.md) - Understand the design and implementation