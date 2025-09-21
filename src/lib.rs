// src/lib/lib.rs

// private modules
mod clock;
mod config;
mod errors;
mod flux_limiter;

// public API exports
pub use clock::{Clock, SystemClock};
pub use config::RateLimiterConfig;
pub use errors::RateLimiterError;
pub use flux_limiter::RateLimiter;

// Available for external users who want to test with it
#[cfg(feature = "testing")]
pub use clock::TestClock;
