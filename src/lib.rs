// src/lib/lib.rs

//! # Flux Limiter
//!
//! A high-performance rate limiter based on the Generic Cell Rate Algorithm (GCRA).
//!
//! ## Quick Example
//!
//! ```rust
//! use flux_limiter::{RateLimiter, RateLimiterConfig, SystemClock};
//!
//! let config = RateLimiterConfig::new(10.0, 5.0);
//! let limiter = RateLimiter::with_config(config, SystemClock).unwrap();
//!
//! if limiter.is_allowed("user_123").unwrap() {
//!     println!("Request allowed");
//! }
//! ```

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
