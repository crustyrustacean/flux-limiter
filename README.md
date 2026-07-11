# Flux Limiter

A simple rate limiter based on the Generic Cell Rate Algorithm (GCRA) with nanosecond precision and lock-free concurrent access.

[![Crates.io](https://img.shields.io/crates/v/flux-limiter)](https://crates.io/crates/flux-limiter) [![Documentation](https://docs.rs/flux-limiter/badge.svg)](https://docs.rs/flux-limiter) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](License.txt) [![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](https://www.rust-lang.org) [![CI](https://img.shields.io/github/actions/workflow/status/crustyrustacean/flux-limiter/ci.yaml)](https://github.com/crustyrustacean/flux-limiter/actions/workflows/ci.yaml)

## Features

- **Mathematically precise**: Implements the GCRA algorithm with exact nanosecond timing
- **Generic client IDs**: Works with any hashable client identifier (`String`, `IpAddr`, `u64`, etc.)
- **Rich metadata**: Returns detailed decision information for HTTP headers, including:
    - `retry_after_seconds`
    - `remaining_capacity`
    - `reset_time_nanos`
- **Memory efficient**: Configurable cleanup of stale client entries
- **Robust error handling**: Graceful handling of clock failures and configuration errors
- **Thread-safe**: Safe concurrent use across multiple threads

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
flux-limiter = "0.8.3"
```

## Quick Start

```rust
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};

// Create a rate limiter: 10 requests per second with allowable burst of 5
let config = FluxLimiterConfig::new(10.0, 5.0);
let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

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

## License

This project is licensed under the MIT License - see the [License.txt](License.txt) file for details.

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.