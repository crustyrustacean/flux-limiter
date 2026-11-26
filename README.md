# Flux Limiter

A simple rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free per-client updates.

[![Crates.io](https://img.shields.io/crates/v/flux-limiter)](https://crates.io/crates/flux-limiter) [![Documentation](https://docs.rs/flux-limiter/badge.svg)](https://docs.rs/flux-limiter) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](License.txt) [![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](https://www.rust-lang.org) [![CI](https://img.shields.io/github/actions/workflow/status/crustyrustacean/flux-limiter/ci.yaml)](https://github.com/crustyrustacean/flux-limiter/actions/workflows/ci.yaml)

## Features

- **Precise**: Implements the GCRA algorithm in integer nanoseconds. The rate is rounded to a whole-nanosecond interval, which is exact for any rate that divides one billion (1, 10, 100, 1e6, ...)
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Rich metadata**: Returns detailed decision information for HTTP headers, including:
    - `retry_after_seconds`
    - `remaining_capacity`
    - `reset_time_nanos`
- **Memory efficient**: One atomic timestamp per client (about 31 bytes on average for a `u64` key), with cleanup of idle clients when you ask for it
- **Robust error handling**: Invalid configuration is rejected with an error, and the default clock cannot fail
- **Thread-safe**: Safe concurrent use across multiple threads. Updating a known client takes no lock
- **Unaffected by clock changes**: The default clock measures elapsed time, so an operating-system clock correction cannot lock clients out

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
flux-limiter = "0.9.0"
```

## Quick Start

```rust
use flux_limiter::{FluxLimiter, FluxLimiterConfig, MonotonicClock};

// Create a rate limiter: 10 requests per second with allowable burst of 5
let config = FluxLimiterConfig::new(10.0, 5.0);
let limiter = FluxLimiter::with_config(config, MonotonicClock::new()).unwrap();

// Check if a request should be allowed
match limiter.check_request("user_123") {
    Ok(decision) => {
        if decision.allowed {
            println!("Request allowed");
        } else {
            println!("Rate limited - retry after {:.2}s",
                     decision.retry_after_seconds.unwrap_or(0.0));
        }
    }
    Err(e) => {
        eprintln!("Rate limiter error: {}", e);
        // Handle error appropriately (e.g., fail-open, fail-closed)
    }
}
```

## Documentation

For comprehensive documentation, including:

- **Error handling strategies** (fail-open, fail-closed, fallback patterns)
- **Configuration guide** (rate/burst explained, builder pattern)
- **Web framework integration** (Axum, Actix, etc.)
- **Advanced usage** (custom client IDs, memory management, cleanup)
- **Production considerations** (monitoring, graceful degradation)
- **Architecture details** (GCRA algorithm, concurrency model, performance)

Please see the [full documentation](https://docs.rs/flux-limiter).

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

What it does not have:

- Weighted checks, async waiting, or `no_std`. `governor` has all three.
- A check that takes the key by reference. `governor` and `quota` do, so a `String` key need not be cloned per request.

## License

This project is licensed under the MIT License - see the [License.txt](License.txt) file for details.

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.