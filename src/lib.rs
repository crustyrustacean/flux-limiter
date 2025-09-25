// src/lib/lib.rs

//! # Flux Limiter
//!
//! A high-performance rate limiter based on the Generic Cell Rate Algorithm (GCRA).
//!
//! ## Quick Example
//!
//! ```rust
//! use flux_limiter::{FluxLimiter, FluxLimiterConfig, SystemClock};
//!
//! let config = FluxLimiterConfig::new(10.0, 5.0);
//! let limiter = FluxLimiter::with_config(config, SystemClock).unwrap();
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
mod config;
mod errors;
mod flux_limiter;
mod clock;

// public API exports
pub use clock::{Clock, SystemClock, ClockError};
pub use config::FluxLimiterConfig;
pub use errors::FluxLimiterError;
pub use flux_limiter::{FluxLimiter, FluxLimiterDecision};
