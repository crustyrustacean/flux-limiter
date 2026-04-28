# Installation

Add Flux Limiter to your Rust project using Cargo.

## Adding to Cargo.toml

Add this to your `Cargo.toml`:

```toml
[dependencies]
flux-limiter = "0.8.0"
```

## Alternative: Using Cargo Add

If you have `cargo-edit` installed, you can use:

```bash
cargo add flux-limiter
```

## Verify Installation

Create a simple example to verify the installation:

```rust
use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};

fn main() {
    let config = FluxLimiterConfig::new(10.0, 5.0);
    let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();

    match limiter.check_request("test_client") {
        Ok(decision) => {
            println!("Request allowed: {}", decision.allowed);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}
```

Run with:

```bash
cargo run
```

You should see output like:

```
Request allowed: true
```

## Next Steps

Now that you have Flux Limiter installed, proceed to:

- [Quick Start](./quick-start.md) - Learn the basics
- [Configuration](./configuration.md) - Understand rate and burst settings
