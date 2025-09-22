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
//! let decision = limiter.check_request("user_123").unwrap();
//! if decision.allowed {
//!     println!("Request allowed");
//! } else {
//!     println!("Rate limited - retry after {:.2}s", 
//!              decision.retry_after_seconds.unwrap_or(0.0));
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
pub use flux_limiter::{RateLimiter, RateLimitDecision};

// Available for external users who want to test with it
#[cfg(feature = "testing")]
pub use clock::TestClock;
